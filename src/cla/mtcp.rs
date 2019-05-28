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

/// MPDU represents a MTCP Data Unit, which will be decoded as a CBOR
/// array of the serialized bundle's length and the serialized bundle.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct MPDU(#[serde(with = "serde_bytes")] ByteBuffer);

impl MPDU {
    pub fn new(bndl: &Bundle) -> MPDU {
        let b = bndl.clone().to_cbor();
        MPDU(b)
    }
}
impl From<MPDU> for bp7::Bundle {
    fn from(item: MPDU) -> Self {
        Bundle::from(item.0)
    }
}
struct MPDUCodec {
    last_pos: usize,
}

impl Encoder for MPDUCodec {
    type Item = MPDU;
    type Error = io::Error;

    fn encode(&mut self, item: MPDU, dst: &mut BytesMut) -> Result<(), io::Error> {
        let buf = serde_cbor::to_vec(&item).unwrap();
        dst.reserve(buf.len());
        dst.put_slice(&buf);
        Ok(())
    }
}

impl Decoder for MPDUCodec {
    type Item = MPDU;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<MPDU>> {
        if buf.len() < 10 {
            // TODO: real minimum size needed
            return Ok(None);
        }
        let res: Result<MPDU, serde_cbor::error::Error> = serde_cbor::from_slice(buf);
        match res {
            Ok(mpdu) => {
                buf.split_to(buf.len());
                self.last_pos = 0;
                Ok(Some(mpdu))
            }
            Err(_) => {
                let pos: Vec<usize> = buf[..]
                    .iter()
                    .enumerate()
                    .filter(|(i, b)| **b == 0x82 && *i > self.last_pos)
                    .map(|(i, _)| i)
                    .collect();
                for p in pos.iter() {
                    let res: Result<MPDU, serde_cbor::error::Error> =
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
pub struct MtcpConversionLayer {
    counter: u64,
}

impl MtcpConversionLayer {
    pub fn new() -> MtcpConversionLayer {
        MtcpConversionLayer { counter: 0 }
    }
    fn spawn_listener(&self) {
        let addr = "0.0.0.0:16162".parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();
        debug!("spawning MTCP listener");
        let server = listener
            .incoming()
            .for_each(move |socket| {
                let peer_addr = socket.peer_addr().unwrap();
                info!("Incoming connection from {}", peer_addr);
                let framed_sock = Framed::new(socket, MPDUCodec { last_pos: 0 });
                let conn = framed_sock
                    .for_each(move |frame| {
                        let bndl = Bundle::from(frame);
                        debug!("Received bundle: {} from {}", bndl.id(), peer_addr);
                        {
                            DTNCORE.lock().unwrap().push(bndl);
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
                    let mpdu = MPDU(b.to_vec());
                    if stream
                        .poll_write(&serde_cbor::to_vec(&mpdu).expect("MPDU encoding error"))
                        .map_err(|err| error!("mtcp write error = {:?}", err))
                        .is_err()
                    {
                        error!("Aborting sending of bundles to {}", addr);
                        break;
                    }
                }
            })
            .map_err(|err| {
                error!("client connect error = {:?}", err);
            });
        tokio::spawn(fut);
    }
}
impl ConvergencyLayerAgent for MtcpConversionLayer {
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
    fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) {
        debug!("Scheduled MTCP submission: {:?}", dest);
        if !ready.is_empty() {
            let addr: IpAddr = dest.parse().unwrap();
            let peeraddr = SocketAddr::new(addr, 16162);
            debug!("forwarding to {:?}", peeraddr);
            self.send_bundles(peeraddr, ready.to_vec());
        } else {
            debug!("Nothing to forward.");
        }
    }
}

impl std::fmt::Display for MtcpConversionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "mtcp")
    }
}
