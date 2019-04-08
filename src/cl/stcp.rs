use crate::bp::{Bp7Error, Bundle, ByteBuffer};
use crate::core::core::{ConversionLayer, DtnCore};
use crate::dtnd::daemon::{access_core, DtnCmd};
use bytes::{BufMut, BytesMut};
use futures::Future;
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use tokio::codec::{Decoder, Encoder, Framed};
use tokio::io;
use tokio::io::AsyncWrite;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

/// DataUnit represents a STCP Data Unit, which will be decoded as a CBOR
/// array of the serialized bundle's length and the serialized bundle.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)] // hacked struct as tuple because bug in serialize_tuple
pub struct DataUnit(usize, #[serde(with = "serde_bytes")] ByteBuffer);

impl DataUnit {
    pub fn new(bndl: &Bundle) -> DataUnit {
        let b = bndl.clone().to_cbor();
        DataUnit(b.len(), b)
    }
    pub fn get_bundle(&self) -> Result<Bundle, Bp7Error> {
        if self.0 != self.1.len() {
            Err(Bp7Error::StcpError(
                "Length variable and bundle's length mismatch".into(),
            ))
        } else {
            Ok(Bundle::from(self.1.clone()))
        }
    }
}

struct DataUnitCodec {
    last_pos: usize,
}

impl Encoder for DataUnitCodec {
    type Item = DataUnit;
    type Error = io::Error;

    fn encode(&mut self, item: DataUnit, dst: &mut BytesMut) -> Result<(), io::Error> {
        let buf = serde_cbor::to_vec(&item).unwrap();
        dst.reserve(buf.len());
        dst.put_slice(&buf);
        Ok(())
    }
}

impl Decoder for DataUnitCodec {
    type Item = DataUnit;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<DataUnit>> {
        if buf.len() < 10 {
            // TODO: real minimum size needed
            return Ok(None);
        }
        let res: Result<DataUnit, serde_cbor::error::Error> = serde_cbor::from_slice(buf);
        match res {
            Ok(spdu) => {
                buf.split_to(buf.len());
                self.last_pos = 0;
                Ok(Some(spdu))
            }
            Err(_) => {
                let pos: Vec<usize> = buf[..]
                    .iter()
                    .enumerate()
                    .filter(|(i, b)| **b == 0x82 && *i > self.last_pos)
                    .map(|(i, _)| i)
                    .collect();
                for p in pos.iter() {
                    let res: Result<DataUnit, serde_cbor::error::Error> =
                        serde_cbor::from_slice(&buf[0..*p]);
                    if res.is_ok() {
                        buf.split_to(*p);
                        self.last_pos = 0;
                        return Ok(Some(res.unwrap()));
                    }
                    self.last_pos = *p;
                }
                Ok(None)
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StcpConversionLayer {
    counter: u64,
    tx: Option<Sender<DtnCmd>>,
}

impl StcpConversionLayer {
    pub fn new() -> StcpConversionLayer {
        StcpConversionLayer {
            counter: 0,
            tx: None,
        }
    }
    fn spawn_listener(&self) {
        let addr = "0.0.0.0:16161".parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();
        debug!("spawning STCP listener");
        let tx = self.tx.clone();
        let server = listener
            .incoming()
            .for_each(move |socket| {
                let tx = tx.clone();
                let peer_addr = socket.peer_addr().unwrap();
                info!("Incoming connection from {}", peer_addr);
                let framed_sock = Framed::new(socket, DataUnitCodec { last_pos: 0 });
                let conn = framed_sock
                    .for_each(move |frame| {
                        debug!(
                            "Received bundle: {} from {}",
                            frame.get_bundle().unwrap().id(),
                            peer_addr
                        );
                        match &tx {
                            Some(tx) => {
                                access_core(tx.clone(), |c| {
                                    c.push(frame.get_bundle().unwrap());
                                });
                            }
                            None => {}
                        }
                        Ok(())
                    })
                    .then(move |_| {
                        info!("Lost connection from {}", peer_addr);
                        Ok(())
                    });
                tokio::spawn(conn);

                Ok(())
            })
            .map_err(|err| {
                error!("accept error = {:?}", err);
            });
        tokio::spawn(server);
    }
    pub fn send_bundles(&self, addr: SocketAddr, bundles: Vec<ByteBuffer>) {
        let stream = TcpStream::connect(&addr);
        let fut = stream
            .map(move |mut stream| {
                // Attempt to write bytes asynchronously to the stream
                for b in &bundles {
                    let spdu = DataUnit(b.len(), b.to_vec());
                    stream
                        .poll_write(&serde_cbor::to_vec(&spdu).unwrap())
                        .map_err(|err| error!("stcp write error = {:?}", err));
                }
            })
            .map_err(|err| {
                error!("client connect error = {:?}", err);
            });
        tokio::spawn(fut);
    }
}
impl ConversionLayer for StcpConversionLayer {
    fn setup(&mut self, tx: Sender<DtnCmd>) {
        debug!("Setup STCP Conversion Layer");
        self.tx = Some(tx);
        self.spawn_listener();
        //self.client_connect("127.0.0.1:16161".parse::<SocketAddr>().unwrap());
        //self.client_connect("127.0.0.1:35037".parse::<SocketAddr>().unwrap());
        let ts = crate::bp::dtntime::CreationTimestamp::with_time_and_seq(
            crate::bp::dtntime::dtn_time_now(),
            0,
        );
        let mut b = crate::bp::helpers::rnd_bundle(ts.clone());
        self.send_bundles(
            "127.0.0.1:16161".parse::<SocketAddr>().unwrap(),
            vec![b.to_cbor(), crate::bp::helpers::rnd_bundle(ts).to_cbor()],
        );
    }
    fn scheduled_send(&self, core: &DtnCore) {
        debug!("Scheduled send STCP Conversion Layer");
    }
}

impl std::fmt::Display for StcpConversionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "StcpConversionLayer")
    }
}
