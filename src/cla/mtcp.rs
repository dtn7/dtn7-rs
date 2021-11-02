use crate::cla::ConvergenceLayerAgent;
use async_trait::async_trait;
use bp7::{Bundle, ByteBuffer};
use bytes::buf::Buf;
use bytes::{BufMut, BytesMut};
use core::convert::TryFrom;
use futures_util::stream::StreamExt;
use lazy_static::lazy_static;
use log::{debug, error, info};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::net::TcpStream;
use std::time::Instant;
use tokio::io;
use tokio::net::TcpListener;
use tokio_util::codec::{Decoder, Encoder, Framed};

lazy_static! {
    pub static ref MTCP_CONNECTIONS: Mutex<HashMap<SocketAddr, TcpStream>> =
        Mutex::new(HashMap::new());
}

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
    let byte_string = 0b0100_0000;
    let type_mask = 0b1110_0000;
    let payload_mask = 0b0001_1111;

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

fn cbor_parse_byte_string_len(input: &[u8]) -> u64 {
    match cbor_parse_byte_string_len_first(input[0]) {
        CborByteString::Len(len) => len as u64,
        CborByteString::U8 => input[1] as u64,
        CborByteString::U16 => ((input[1] as u64) << 8) + (input[2] as u64),
        CborByteString::U32 => {
            ((input[1] as u64) << 24)
                + ((input[2] as u64) << 16)
                + ((input[3] as u64) << 8)
                + (input[4] as u64)
        }
        CborByteString::U64 => {
            ((input[1] as u64) << 56)
                + ((input[2] as u64) << 48)
                + ((input[3] as u64) << 40)
                + ((input[4] as u64) << 32)
                + ((input[5] as u64) << 24)
                + ((input[6] as u64) << 16)
                + ((input[7] as u64) << 8)
                + (input[8] as u64)
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

impl TryFrom<MPDU> for bp7::Bundle {
    type Error = bp7::error::Error;
    fn try_from(item: MPDU) -> Result<Self, Self::Error> {
        Bundle::try_from(item.0)
    }
}

pub struct MPDUCodec {
    last_pos: usize,
}

impl MPDUCodec {
    pub fn new() -> MPDUCodec {
        MPDUCodec { last_pos: 0 }
    }
}

impl Default for MPDUCodec {
    fn default() -> Self {
        Self::new()
    }
}
impl Encoder<MPDU> for MPDUCodec {
    type Error = io::Error;

    fn encode(&mut self, item: MPDU, dst: &mut BytesMut) -> Result<(), Self::Error> {
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
        if let Some(expected_pos) =
            cbor_hdr_len(buf[0]).checked_add(cbor_parse_byte_string_len(&buf[0..10]) as usize)
        {
            if let Some(expected_pos) = expected_pos.checked_sub(1) {
                if expected_pos < buf.len() {
                    if 0xff != buf[expected_pos] {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid MPDU data (terminator not found)",
                        ));
                    }
                    if let Ok(res) = serde_cbor::from_slice(&buf[0..=expected_pos]) {
                        buf.advance(expected_pos + 1);
                        self.last_pos = 0;
                        return Ok(Some(res));
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Invalid MPDU data (decoding error)",
                        ));
                    }
                }
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid MPDU data (position overflow)",
                ));
            }
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid MPDU data (position overflow)",
            ));
        }
        Ok(None)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct MtcpConvergenceLayer {
    counter: u64,
    local_port: u16,
}

impl MtcpConvergenceLayer {
    pub fn new(port: Option<u16>) -> MtcpConvergenceLayer {
        MtcpConvergenceLayer {
            counter: 0,
            local_port: port.unwrap_or(16162),
        }
    }
    async fn handle_connection(socket: tokio::net::TcpStream) -> anyhow::Result<()> {
        let peer_addr = socket.peer_addr().unwrap();
        info!("Incoming connection from {}", peer_addr);
        let mut framed_sock = Framed::new(socket, MPDUCodec::new());
        while let Some(frame) = framed_sock.next().await {
            match frame {
                Ok(frame) => {
                    if let Ok(bndl) = Bundle::try_from(frame) {
                        info!("Received bundle: {} from {}", bndl.id(), peer_addr);
                        {
                            tokio::spawn(async move {
                                if let Err(err) = crate::core::processing::receive(bndl).await {
                                    error!("Failed to process bundle: {}", err);
                                }
                            });
                        }
                    } else {
                        info!("Error decoding bundle from {}", peer_addr);
                        break;
                    }
                }
                Err(err) => {
                    info!("Lost connection from {} ({})", peer_addr, err);
                    break;
                }
            }
        }
        info!("Disconnected {}", peer_addr);
        Ok(())
    }
    async fn listener(self) -> Result<(), io::Error> {
        let port = self.port();
        let addr: SocketAddrV4 = format!("0.0.0.0:{}", port).parse().unwrap();
        let listener = TcpListener::bind(&addr)
            .await
            .expect("failed to bind tcp port");
        debug!("spawning MTCP listener on port {}", port);
        loop {
            let (socket, _) = listener.accept().await.unwrap();

            tokio::spawn(MtcpConvergenceLayer::handle_connection(socket));
        }
    }
    pub async fn spawn_listener(&self) -> std::io::Result<()> {
        // TODO: bubble up errors from run
        tokio::spawn(self.listener()); /*.await.unwrap()*/
        Ok(())
    }
    pub fn send_bundles(&self, addr: SocketAddr, bundles: Vec<ByteBuffer>) -> bool {
        // TODO: implement correct error handling
        // TODO: classic sending thread, tokio code would block and not complete large transmissions
        let now = Instant::now();
        let num_bundles = bundles.len();
        let mut buf = Vec::new();
        for b in bundles {
            let mpdu = MPDU(b);
            if let Ok(buf2) = serde_cbor::to_vec(&mpdu) {
                buf.extend_from_slice(&buf2);
            } else {
                error!("MPDU encoding error!");
                return false;
            }
        }

        #[allow(clippy::map_entry)]
        if !MTCP_CONNECTIONS.lock().contains_key(&addr) {
            debug!("Connecting to {}", addr);
            if let Ok(stream) = TcpStream::connect(&addr) {
                MTCP_CONNECTIONS.lock().insert(addr, stream);
            } else {
                error!("Error connecting to remote {}", addr);
                return false;
            }
        } else {
            debug!("Already connected to {}", addr);
        };
        let mut s1 = MTCP_CONNECTIONS
            .lock()
            .get(&addr)
            .unwrap()
            .try_clone()
            .unwrap();

        if s1.write_all(&buf).is_err() {
            error!("Error writing data to {}", addr);
            MTCP_CONNECTIONS.lock().remove(&addr);
            return false;
        }
        info!(
            "Transmission time: {:?} for {} bundles in {} bytes to {}",
            now.elapsed(),
            num_bundles,
            buf.len(),
            addr
        );

        true
    }
}

#[async_trait]
impl ConvergenceLayerAgent for MtcpConvergenceLayer {
    async fn setup(&mut self) {
        self.spawn_listener()
            .await
            .expect("error setting up mtcp listener");
    }
    fn port(&self) -> u16 {
        self.local_port
    }
    fn name(&self) -> &'static str {
        "mtcp"
    }
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
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

impl std::fmt::Display for MtcpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "mtcp:{}", self.local_port)
    }
}
