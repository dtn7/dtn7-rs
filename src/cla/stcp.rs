use crate::cla::ConvergencyLayerAgent;
use crate::DTNCORE;
use bp7::{Bp7Error, Bundle, ByteBuffer};
use bytes::{BufMut, BytesMut};
use futures::Future;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::net::SocketAddr;
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
}

impl StcpConversionLayer {
    pub fn new() -> StcpConversionLayer {
        StcpConversionLayer { counter: 0 }
    }
    fn spawn_listener(&self) {
        let addr = "0.0.0.0:16161".parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();
        debug!("spawning STCP listener");
        let server = listener
            .incoming()
            .for_each(move |socket| {
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
                        {
                            DTNCORE.lock().unwrap().push(frame.get_bundle().unwrap());
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
impl ConvergencyLayerAgent for StcpConversionLayer {
    fn setup(&mut self) {
        self.spawn_listener();

        // TODO: remove the following test code
        /*self.send_bundles(
            "127.0.0.1:16161".parse::<SocketAddr>().unwrap(),
            vec![
                bp7::helpers::rnd_bundle(CreationTimestamp::now()).to_cbor(),
                bp7::helpers::rnd_bundle(CreationTimestamp::now()).to_cbor(),
                bp7::helpers::rnd_bundle(CreationTimestamp::now()).to_cbor(),
            ],
        );*/
    }
    fn scheduled_submission(&self, ready: &[ByteBuffer], dest: &String) {
        debug!("Scheduled STCP submission: {:?}", dest);
        if !ready.is_empty() {
            let addr: IpAddr = dest.parse().unwrap();
            let peeraddr = SocketAddr::new(addr, 16161);
            debug!("forwarding to {:?}", peeraddr);
            self.send_bundles(peeraddr, ready.to_vec());
        } else {
            debug!("Nothing to forward.");
        }
    }
}

impl std::fmt::Display for StcpConversionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "StcpConversionLayer")
    }
}
