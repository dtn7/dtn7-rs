use super::ConvergenceLayerAgent;
use async_trait::async_trait;
use bitflags::*;
use bp7::ByteBuffer;
//use futures_util::stream::StreamExt;
use log::{debug, error, info};
use std::io::Write;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
//use std::net::TcpStream;
use bytes::Bytes;
use std::time::Instant;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use num_derive::*;
use num_traits::FromPrimitive;
use anyhow::{bail, anyhow};
use std::convert::TryInto;
use std::io::Cursor;


bitflags! {
    /// Contact Header flags
    #[derive(Default)]
    struct ContactHeaderFlags : u8 {
        const CAN_TLS = 0x01;
    }
}

/// Message Types
#[derive(Debug, FromPrimitive)]
#[repr(u8)]
enum MessageType {
    /// Indicates the transmission of a segment of bundle data.
    XFER_SEGMENT = 0x01,
    /// Acknowledges reception of a data segment.
    XFER_ACK = 0x02,
    /// Indicates that the transmission of the current bundle SHALL be stopped.
    XFER_REFUSE = 0x03,
    /// Used to keep TCPCL session active.
    KEEPALIVE = 0x04,
    /// Indicates that one of the entities participating in the session wishes to cleanly terminate the session.
    SESS_TERM = 0x05,
    /// Contains a TCPCL message rejection.
    MSG_REJECT = 0x06,
    /// Contains the session parameter inputs from one of the entities.
    SESS_INIT = 0x07,
}

bitflags! {
    /// Session Extension Item flags
    struct SessionExtensionItemFlags : u8 {
        const CRITICAL = 0x01;
    }
}

/// MSG_REJECT Reason Codes
#[derive(Debug, FromPrimitive)]
#[repr(u8)]
enum MsgRejectReasonCode {
    /// A message was received with a Message Type code unknown to the TCPCL node.
    MessageTypeUnknown = 0x01,
    /// A message was received but the TCPCL entity cannot comply with the message contents.
    MessageUnsupported = 0x02,
    /// A message was received while the session is in a state in which the message is not expected.
    MessageUnexpected = 0x03,
}

bitflags! {
    /// XFER_SEGMENT flags
    struct XferSegmentFlags : u8 {
        const END = 0x01;
        const START = 0x02;
    }
}

/// XFER_REFUSE Reason Codes
#[derive(Debug, FromPrimitive)]
#[repr(u8)]
enum XferRefuseReasonCode {
    /// Reason for refusal is unknown or not specified.
    Unknown = 0,
    /// The receiver already has the complete bundle. The sender MAY consider the bundle as completely received.
    Completed = 0x01,
    /// The receiver's resources are exhausted. The sender SHOULD apply reactive bundle fragmentation before retrying.
    NoResources = 0x02,
    /// The receiver has encountered a problem that requires the bundle to be retransmitted in its entirety.
    Retransmit = 0x03,
    /// Some issue with the bundle data or the transfer extension data was encountered. The sender SHOULD NOT retry the same bundle with the same extensions.
    NotAcceptable = 0x04,
    /// A failure processing the Transfer Extension Items has occurred.
    ExtensionFailure = 0x05,
}

bitflags! {
    /// Transfer Extension Item flags
    struct TransferExtensionItemFlags : u8 {
        const CRITICAL = 0x01;
    }
}

bitflags! {
    /// SESS_TERM flags
    struct SessTermFlags : u8 {
        /// If bit is set, indicates that this message is an acknowledgement of an earlier SESS_TERM message.
        const REPLY = 0x01;
    }
}


/// SESS_TERM Reason Codes
#[derive(Debug, FromPrimitive)]
#[repr(u8)]
enum SessTermReasonCode {
    /// A termination reason is not available.
    Unknown = 0,
    /// The session is being closed due to idleness.
    IdleTimeout = 0x01,

    VersionMismatch = 0x02,

    Busy = 0x03,

    ContactFailure = 0x04,

    ResourceExhaustion = 0x05,
}

struct SessInitData {
    keepalive: u16,
    segment_mru: u64,
    transfer_mru: u64,
    node_id: String,
}

struct XferAckData {
    flags : XferSegmentFlags,
    tid : u64,
    len : u64,
}
struct XferRefuseData {
    reason : XferRefuseReasonCode,
    tid : u64,
}
struct SessTermData {
    flags : SessTermFlags,
    reason : SessTermReasonCode,
}

enum TcpClPacket  {
    SessInit(SessInitData),
    SessTerm(SessTermData),
    XferSeg,
    XferAck(XferAckData),
    XferRefuse(XferRefuseData),
    KeepAlive,
    MsgReject,
}

struct Connection {
    stream: TcpStream,
    buffer: bytes::BytesMut,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream,
            //buffer: bytes::BytesMut::with_capacity(4096*64),
            buffer: bytes::BytesMut::with_capacity(4096),
        }
    }
}

async fn parses_packet(mut buffer : bytes::BytesMut) -> anyhow::Result<TcpClPacket> {
    let mut buf = Cursor::new(&buffer[..]);

    let mtype = buf.read_u8().await?;
    if let  Some(mtype)= MessageType::from_u8(mtype) {
        match  mtype {
            MessageType::XFER_SEGMENT => { // TODO
                Ok(TcpClPacket::XferSeg)
            },
            MessageType::XFER_ACK => {
                if buffer.len() < 18 {
                    bail!("Not enough bytes received");
                }
                /*let flags = XferSegmentFlags::from_bits_truncate(buf[1]);
                let tid : u64 = u64::from_be_bytes(buf[2..10].try_into()?);
                let len : u64 = u64::from_be_bytes(buf[10..18].try_into()?);*/
                let flags = XferSegmentFlags::from_bits_truncate(buf.read_u8().await?);
                let tid : u64 = buf.read_u64().await?;
                let len : u64 = buf.read_u64().await?;
                let data = XferAckData {
                    flags,
                    tid,
                    len,
                };
                let pkt_len = buf.position() as usize;
                buffer.truncate(pkt_len);
                Ok(TcpClPacket::XferAck(data))
            },
            MessageType::XFER_REFUSE => {
                if buffer.len() < 10 {
                    bail!("Not enough bytes received");
                }
                if let Some(reason) = XferRefuseReasonCode::from_u8(buf.read_u8().await?) {
                    let tid : u64 = buf.read_u64().await?;
                    let data = XferRefuseData {
                        reason,
                        tid,
                    };
                    let pkt_len = buf.position() as usize;
                    buffer.truncate(pkt_len);
                    Ok(TcpClPacket::XferRefuse(data))

                } else {
                    bail!("Unknown reason code in xfer refuse message");
                }
            },
            MessageType::KEEPALIVE => {Ok(TcpClPacket::KeepAlive)},
            MessageType::SESS_TERM => {
                if buffer.len() < 3 {
                    bail!("Not enough bytes received");
                }
                let flags = SessTermFlags::from_bits_truncate(buf.read_u8().await?);
                if let Some(reason) = SessTermReasonCode::from_u8(buf.read_u8().await?) {
                    let data = SessTermData {
                        flags,
                        reason,
                    };
                    let pkt_len = buf.position() as usize;
                    buffer.truncate(pkt_len);
                    Ok(TcpClPacket::SessTerm(data))

                } else {
                    bail!("Unknown reason code in sess term message");

                }
            },
            MessageType::SESS_INIT => { // TODO
                let data = SessInitData {
                    keepalive: 0,
                    segment_mru: 0,
                    transfer_mru: 0,
                    node_id: "nonode".into(),
                };
                Ok(TcpClPacket::SessInit(data))},
            MessageType::MSG_REJECT => {
                // TODO
                Ok(TcpClPacket::MsgReject)
            },
        }    
    } else {
        // unknown  code
        bail!("Unknown packet type");
    }
    
    
}

#[derive(Debug, Clone, Default, Copy)]
pub struct TcpConvergenceLayer {
    counter: u64,
    local_port: u16,
}

impl TcpConvergenceLayer {
    pub fn new(port: Option<u16>) -> TcpConvergenceLayer {
        TcpConvergenceLayer {
            counter: 0,
            local_port: port.unwrap_or(4556),
        }
    }
    async fn run(self) -> Result<(), io::Error> {
        let addr: SocketAddrV4 = format!("0.0.0.0:{}", self.port()).parse().unwrap();
        let mut listener = TcpListener::bind(&addr).await?;
        //tokio::spawn({ client_connect("127.0.0.1:4223".parse().unwrap()) });
        debug!("spawning TCP listener on port {}", self.port(),);
        loop {
            let (mut socket, remote) = listener
                .accept()
                .await
                .expect("error accepting TCPCL connection");

            let peer_addr = socket.peer_addr().unwrap();
            info!("Incoming connection from {}", peer_addr);

            // Phase 1: Exchange Contact Header
            debug!("CH <-");
            let res = self.receive_contact_header(&mut socket).await;
            if res.is_err() {
                debug!("received error: {:?}", res);
                self.send_sess_term(
                    &mut socket,
                    SessTermReasonCode::VersionMismatch,
                    SessTermFlags::empty(),
                )
                .await;
                continue;
            }
            let ch_flags = res.unwrap();
            debug!("CH ->");
            if self.send_contact_header(&mut socket).await.is_err() {
                continue;
            };

            debug!("exchanged contact headers");
            // Phase 2: Negotiate

            let sess_init_data = SessInitData {
                keepalive: 30,
                segment_mru: 64000,
                transfer_mru: 64000,
                node_id: "test2".into(),
            };
            self.send_sess_init(&mut socket, sess_init_data).await;

            // Phase 3: Idle

            // Phase 4: Teardown

            /*let mut framed_sock = Framed::new(socket, MPDUCodec::new());
            while let Some(frame) = framed_sock.next().await {
                match frame {
                    Ok(frame) => {
                        if let Ok(bndl) = Bundle::try_from(frame) {
                            info!("Received bundle: {} from {}", bndl.id(), peer_addr);
                            {
                                std::thread::spawn(move || {
                                    crate::core::processing::receive(bndl.into());
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
            }*/
            info!("Disconnected {}", peer_addr);
        }
    }
    async fn send_contact_header(&self, socket: &mut TcpStream) -> anyhow::Result<()> {
        let ch_flags: ContactHeaderFlags = Default::default();
        socket.write(b"dtn!").await?;
        socket.write_u8(4).await?;
        socket.write_u8(ch_flags.bits()).await?;
        socket.flush().await?;
        Ok(())
    }
    async fn send_sess_term(
        &self,
        socket: &mut TcpStream,
        reason: SessTermReasonCode,
        flags: SessTermFlags,
    ) -> anyhow::Result<()> {
        socket.write_u8(MessageType::SESS_TERM as u8).await?;
        socket.write_u8(flags.bits()).await?;
        socket.write_u8(reason as u8).await?;
        socket.flush().await?;
        Ok(())
    }
    async fn send_sess_init(
        &self,
        socket: &mut TcpStream,
        data: SessInitData,
    ) -> anyhow::Result<()> {
        socket.write_u8(MessageType::SESS_INIT as u8).await?;
        socket.write_u16(data.keepalive).await?;
        socket.write_u64(data.segment_mru).await?;
        socket.write_u64(data.transfer_mru).await?;
        socket.write_u16(data.node_id.len() as u16).await?;
        socket.write_all(data.node_id.as_bytes()).await?;
        socket.write_u32(0).await?;
        socket.flush().await?;
        Ok(())
    }
    async fn send_keepalive(&self, socket: &mut TcpStream) -> anyhow::Result<()> {
        socket.write_u8(MessageType::KEEPALIVE as u8).await?;
        socket.flush().await?;
        Ok(())
    }
    async fn send_xfer_segment(
        &self,
        socket: &mut TcpStream,
        flags: XferSegmentFlags,
        transfer_id: u64,
        ack_len: u64,
    ) -> anyhow::Result<()> {
        socket.write_u8(MessageType::XFER_SEGMENT as u8).await?;
        socket.write_u8(flags.bits()).await?;
        socket.write_u64(transfer_id).await?;
        socket.write_u64(ack_len).await?;
        socket.flush().await?;
        Ok(())
    }
    async fn send_xfer_ack(
        &self,
        socket: &mut TcpStream,
        flags: XferSegmentFlags,
        transfer_id: u64,
        data: Bytes,
    ) -> anyhow::Result<()> {
        socket.write_u8(MessageType::XFER_ACK as u8).await?;
        socket.write_u8(flags.bits()).await?;
        socket.write_u64(transfer_id).await?;
        socket.write_u32(0).await?;
        socket.write_u64(data.len() as u64).await?;
        socket.write_all(&data).await?;
        socket.flush().await?;
        Ok(())
    }
    async fn send_xfer_refuse(
        &self,
        socket: &mut TcpStream,
        reason: XferRefuseReasonCode,
        transfer_id: u64,
    ) -> anyhow::Result<()> {
        socket.write_u8(MessageType::XFER_REFUSE as u8).await?;
        socket.write_u8(reason as u8).await?;
        socket.write_u64(transfer_id).await?;
        socket.flush().await?;
        Ok(())
    }

    async fn receive_contact_header(
        &self,
        socket: &mut TcpStream,
    ) -> anyhow::Result<ContactHeaderFlags> {
        let mut buf: [u8; 6] = [0; 6];
        //let ch_flags: ContactHeaderFlags = Default::default();
        socket.read_exact(&mut buf).await?;

        if &buf[0..4] != b"dtn!" {
            anyhow::bail!("Invalid magic");
        }

        if buf[4] != 4 {
            anyhow::bail!("Unsupported version");
        }

        Ok(ContactHeaderFlags::from_bits_truncate(buf[5]))
    }
    pub async fn spawn_listener(&self) -> std::io::Result<()> {
        tokio::spawn(self.run());
        Ok(())
    }
    pub async fn connect(&self, addr: SocketAddr) -> anyhow::Result<()> {
        debug!("client connecting via stream");
        if let Ok(mut stream) = TcpStream::connect(&addr).await {
            debug!("sending CH");
            self.send_contact_header(&mut stream).await?;

            debug!("receiving CH");
            let res = self.receive_contact_header(&mut stream).await;
            if res.is_err() {
                debug!("received error: {:?}", res);
                self.send_sess_term(
                    &mut stream,
                    SessTermReasonCode::VersionMismatch,
                    SessTermFlags::empty(),
                )
                .await;
            } else {
                let ch_flags = res.unwrap();

                debug!("got flags: {:?}", ch_flags);

                let sess_init_data = SessInitData {
                    keepalive: 30,
                    segment_mru: 64000,
                    transfer_mru: 64000,
                    node_id: "test1".into(),
                };
                self.send_sess_init(&mut stream, sess_init_data).await?;
            }
        } else {
            debug!("error connceting to peer: {:?}", addr);
        }
        Ok(())
    }
    pub fn send_bundles(&self, addr: SocketAddr, bundles: Vec<ByteBuffer>) -> bool {
        // TODO: implement correct error handling
        // TODO: classic sending thread, tokio code would block and not complete large transmissions
        //thread::spawn(move || {
        /*let now = Instant::now();
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
        }*/
        //});
        //tokio::spawn(async move {
        //    self.connect(addr).await;
        //});
        //self.connect(addr).await;
        true
    }
}
#[async_trait]
impl ConvergenceLayerAgent for TcpConvergenceLayer {
    async fn setup(&mut self) {
        self.spawn_listener()
            .await
            .expect("error setting up tcp listener");
    }

    fn port(&self) -> u16 {
        self.local_port
    }
    fn name(&self) -> &'static str {
        "tcp"
    }
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
        debug!("Scheduled TCP submission: {:?}", dest);
        if !ready.is_empty() {
            let peeraddr: SocketAddr = dest.parse().unwrap();
            debug!("forwarding to {:?}", peeraddr);
            //return self.send_bundles(peeraddr, ready.to_vec());
            //self.connect(peeraddr);
            //let rt = tokio::runtime::Runtime::new().unwrap();
            //let rt = tokio::runtime::Handle::current();
            //rt.spawn(async move {
            //client_connect(peeraddr).await;
            self.connect(peeraddr).await;
        //});
        } else {
            debug!("Nothing to forward.");
        }
        true
    }
}

impl std::fmt::Display for TcpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "tcp")
    }
}
