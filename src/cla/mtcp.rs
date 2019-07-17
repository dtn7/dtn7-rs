use crate::cla::ConvergencyLayerAgent;
use bp7::{Bundle, ByteBuffer};
use bytes::{BufMut, BytesMut};
use futures::Future;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::net::TcpStream;
use std::time::Instant;
use tokio::codec::{Decoder, Encoder, Framed};
use tokio::io;
use tokio::net::TcpListener;
use tokio::prelude::*;

#[derive(Debug)]
enum CborByteString {
    Len(u8),
    U8,
    U16,
    U32,
    U64,
    Not,
}
fn cbor_parse_byte_string_len_first(input: u8) -> CborByteString {
    let byte_string = 0b010_00000;
    let type_mask = 0b111_00000;
    let payload_mask = 0b000_11111;

    if input & type_mask != byte_string {
        return CborByteString::Not;
    }

    let number = input & payload_mask;

    if number < 24 {
        CborByteString::Len(number)
    } else if number == 24 {
        CborByteString::U8
    } else if number == 25 {
        CborByteString::U16
    } else if number == 26 {
        CborByteString::U32
    } else if number == 27 {
        CborByteString::U64
    } else {
        CborByteString::Not
    }
}
fn cbor_hdr_len(input: u8) -> usize {
    match cbor_parse_byte_string_len_first(input) {
        CborByteString::Len(_) => 1,
        CborByteString::U8 => 2,
        CborByteString::U16 => 3,
        CborByteString::U32 => 5,
        CborByteString::U64 => 9,
        _ => 0,
    }
}
fn cbor_parse_byte_string_len(input: &[u8]) -> usize {
    match cbor_parse_byte_string_len_first(input[0]) {
        CborByteString::Len(len) => len as usize,
        CborByteString::U8 => input[1] as usize,
        CborByteString::U16 => ((input[1] as usize) << 8) + (input[2] as usize),
        CborByteString::U32 => {
            ((input[1] as usize) << 24)
                + ((input[2] as usize) << 16)
                + ((input[3] as usize) << 8)
                + (input[4] as usize)
        }
        CborByteString::U64 => {
            ((input[1] as usize) << 56)
                + ((input[2] as usize) << 48)
                + ((input[3] as usize) << 40)
                + ((input[4] as usize) << 32)
                + ((input[5] as usize) << 24)
                + ((input[6] as usize) << 16)
                + ((input[7] as usize) << 8)
                + (input[8] as usize)
        }
        _ => 0,
    }
}

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
        if cbor_hdr_len(buf[0]) == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid MPDU data (length)",
            ));
        };
        let expected_pos = cbor_hdr_len(buf[0]) + cbor_parse_byte_string_len(&buf[0..10]) - 1;
        if expected_pos < buf.len() {
            if 0xff != buf[expected_pos] {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid MPDU data (terminator not found)",
                ));
            }
            let res: Result<MPDU, serde_cbor::error::Error> =
                serde_cbor::from_slice(&buf[0..=expected_pos]);
            if res.is_ok() {
                buf.split_to(expected_pos + 1);
                self.last_pos = 0;
                return Ok(Some(res.unwrap()));
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid MPDU data (decoding error)",
                ));
            }
        }
        Ok(None)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MtcpConversionLayer {
    counter: u64,
    local_port: u16,
}

impl MtcpConversionLayer {
    pub fn new(port: Option<u16>) -> MtcpConversionLayer {
        MtcpConversionLayer {
            counter: 0,
            local_port: port.unwrap_or(16162),
        }
    }
    fn spawn_listener(&self) {
        let addr = format!("0.0.0.0:{}", self.port()).parse().unwrap();
        let listener = TcpListener::bind(&addr).unwrap();
        debug!("spawning MTCP listener on port {}", self.local_port);
        let server = listener
            .incoming()
            .for_each(move |socket| {
                let peer_addr = socket.peer_addr().unwrap();
                info!("Incoming connection from {}", peer_addr);
                let framed_sock = Framed::new(socket, MPDUCodec { last_pos: 0 });
                let conn = framed_sock
                    .for_each(move |frame| {
                        let bndl = Bundle::from(frame);
                        info!("Received bundle: {} from {}", bndl.id(), peer_addr);
                        {
                            //DTNCORE.lock().unwrap().push(bndl);
                            crate::core::processing::receive(bndl.into());
                        }

                        Ok(())
                    })
                    .map_err(move |err| info!("Lost connection from {} ({})", peer_addr, err))
                    .then(move |_| {
                        info!("Disconnected {}", peer_addr);
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
    pub fn send_bundles(&self, addr: SocketAddr, bundles: Vec<ByteBuffer>) -> bool {
        // TODO: implement correct error handling
        // TODO: classic sending thread, tokio code would block and not complete large transmissions
        //thread::spawn(move || {
        let now = Instant::now();
        let num_bundles = bundles.len();
        let mut buf = Vec::new();
        for b in bundles {
            let mpdu = MPDU(b);
            let buf2 = serde_cbor::to_vec(&mpdu);
            if buf2.is_err() {
                error!("MPDU encoding error!");
                return false;
            } else {
                buf.extend_from_slice(&buf2.unwrap());
            }
        }
        if let Ok(mut s1) = TcpStream::connect(&addr) {
            if s1.write_all(&buf).is_err() {
                error!("Error writing data to {}", addr);
                return false;
            }
            info!(
                "Transmission time: {:?} for {} bundles in {} bytes to {}",
                now.elapsed(),
                num_bundles,
                buf.len(),
                addr
            );
        } else {
            error!("Error connecting to remote {}", addr);
            return false;
        }
        //});
        true
    }
}
impl ConvergencyLayerAgent for MtcpConversionLayer {
    fn setup(&mut self) {
        self.spawn_listener();
    }
    fn port(&self) -> u16 {
        self.local_port
    }
    fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
        debug!("Scheduled MTCP submission: {:?}", dest);
        if !ready.is_empty() {
            let peeraddr: SocketAddr = dest.parse().unwrap();
            debug!("forwarding to {:?}", peeraddr);
            return self.send_bundles(peeraddr, ready.to_vec());
        } else {
            debug!("Nothing to forward.");
        }
        true
    }
}

impl std::fmt::Display for MtcpConversionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "mtcp:{}", self.local_port)
    }
}
