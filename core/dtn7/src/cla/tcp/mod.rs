pub mod net;
pub mod proto;

use self::net::*;

use super::{ConvergenceLayerAgent, HelpStr, TransferResult};
use async_trait::async_trait;
use bp7::{Bundle, ByteBuffer, EndpointID};
//use futures_util::stream::StreamExt;
use dtn7_codegen::cla;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::time::Instant;
use thiserror::Error;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::{self, Sender};
use tokio::sync::Mutex;
use tokio::time::{self};
//use std::net::TcpStream;
use super::tcp::proto::*;
use crate::core::store::BundleStore;
use crate::core::PeerType;
use crate::{peers_add, peers_known, STORE};
use crate::{DtnPeer, CONFIG};
use anyhow::bail;
use bytes::Bytes;
use lazy_static::lazy_static;
use tokio::io::{AsyncReadExt, BufReader, BufWriter};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::Duration;

// TODO
// Implemented draft version 24
// sending/receiving of bundles, always uses maximum allowed packet size, no segmentation
// ssl not implemented yet

/*
    There is one TcpConvergenceLayer object that spawns one Listener task.
    The convergence layer holds all currently active TCPCL sessions.
    A new session is established by either receiving a new connection in the Listener or by sending bundles to a new destination.
    The session is established by first creating a TcpConnection, exchanging session information and then transitioning to a TcpSession.
    Per session a sending and receiving task exist, encapsulating the respective parts of the tcp connection.
    A third TcpSession task maintains session state and sends/receives bundles. TcpConvergenceLayer communicates via channels with TcpSession.
*/

type SessionMap = HashMap<SocketAddr, mpsc::Sender<(ByteBuffer, oneshot::Sender<TransferResult>)>>;

const KEEPALIVE: u16 = 30;
const SEGMENT_MRU: u64 = 64000;
const TRANSFER_MRU: u64 = 64000;
const INTERNAL_CHANNEL_BUFFER: usize = 200;

lazy_static! {
    pub static ref TCP_CONNECTIONS: Mutex<SessionMap> = Mutex::new(HashMap::new());
}

#[derive(Error, Debug)]
enum TcpSessionError {
    #[error("Internal channel send error")]
    InternalChannel(#[from] SendError<TcpClPacket>),
    #[error("Result channel send error")]
    ResultChannel,
    #[error("Protocol error: {0:?}")]
    Protocol(TcpClPacket),
}

impl From<TransferResult> for TcpSessionError {
    fn from(_: TransferResult) -> Self {
        TcpSessionError::ResultChannel
    }
}

/// Initial tcp connection.
/// Session not yet established.
struct TcpConnection {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
    addr: SocketAddr,
    refuse_existing_bundles: bool,
}

struct TcpSession {
    reader: BufReader<OwnedReadHalf>,
    writer: BufWriter<OwnedWriteHalf>,
    addr: SocketAddr,
    refuse_existing_bundles: bool,
    remote_session_data: SessInitData,
    _local_session_data: SessInitData,
    last_tid: u64,
    rx_session_queue: mpsc::Receiver<(Vec<u8>, Sender<TransferResult>)>,
}

enum ReceiveState {
    Idle,
    Receiving(Vec<u8>, u64),
    Terminated,
}

enum SendState {
    Idle,
    Sending(u64, tokio::sync::oneshot::Sender<TransferResult>),
    TransferRequest(Vec<u8>, tokio::sync::oneshot::Sender<TransferResult>),
    Terminated,
}

impl TcpSession {
    async fn run(mut self) {
        let mut keepalive_sent = false;
        let mut keepalive_received = false;
        let mut state = (ReceiveState::Idle, SendState::Idle);
        loop {
            if matches!(state.1, SendState::Terminated)
                || matches!(state.0, ReceiveState::Terminated)
            {
                info!(
                    "Session terminated for {} ({})",
                    self.remote_session_data.node_id, self.addr
                );
                break;
            }
            // timeout send keepalive/send packet
            // timeout receive keepalive/receive packet
            // select!
            // if first task completes first, receiving timeout is cancelled
            // but because we await an ack or some sort of response anyway, this doesn't matter
            // the timeout is respected in send()

            // if second task completes first, sending timeout is cancelled
            // but we will send response packets anyway
            // if keepalive is received, just answer with a keepalive anyway
            let sleep = time::sleep(Duration::from_secs(
                self.remote_session_data.keepalive.into(),
            ));
            tokio::pin!(sleep);
            tokio::select! {
                received_packet = TcpClPacket::read(&mut self.reader) => {
                    match received_packet {
                        Ok(packet) => {
                            if packet == TcpClPacket::KeepAlive {
                                if !keepalive_sent {
                                    if let Err(err) = TcpClPacket::KeepAlive.write(&mut self.writer).await {
                                        error!("error while sending keepalive: {:?}", err);
                                        state = (ReceiveState::Terminated, SendState::Terminated);
                                    }
                                    keepalive_sent = true;
                                } else {
                                    keepalive_sent = false;
                                }
                                keepalive_received = true;
                            } else {
                                match self.receive(packet, state).await {
                                    Err(err) => {
                                        error!("error while receiving: {:?}",err);
                                        state = (ReceiveState::Terminated, SendState::Terminated);
                                    }
                                    Ok(new_state) => state = new_state
                                }
                                keepalive_received = false;
                                keepalive_sent = false;
                            }
                        },
                        Err(err) => {
                            error!("Failed parsing package: {:?}", err);
                            state = (ReceiveState::Terminated, SendState::Terminated);
                        },
                    }
                }
                queue_bundle = self.rx_session_queue.recv(), if matches!(state.1, SendState::Idle) => {
                    match queue_bundle {
                        Some(bundle) => {
                            match self.send(bundle).await {
                                Err(err) => {
                                    error!("error while sending: {:?}", err);
                                    state = (ReceiveState::Terminated, SendState::Terminated);
                                }
                                Ok(new_state) => state.1 = new_state
                            }
                        },
                        None => {
                            // session closed by closing internal channel
                            state = self.terminate_session(SessTermReasonCode::Unknown).await;
                        },
                    }
                    keepalive_sent = false;
                    keepalive_received = false;
                }
                _ = sleep => {
                    if !keepalive_sent {
                        // 1st time send keepalive
                        if let Err(err) = TcpClPacket::KeepAlive.write(&mut self.writer).await {
                            error!("error while sending keepalive: {:?}", err);
                            state = (ReceiveState::Terminated, SendState::Terminated);
                        }
                        keepalive_sent = true;
                    }
                    if !keepalive_received && keepalive_sent{
                        // 2nd time terminate session
                        state = self.terminate_session(SessTermReasonCode::IdleTimeout).await;
                    }
                }
                else => {
                    error!("all channels closed");
                    state = (ReceiveState::Terminated, SendState::Terminated);
                }
            };
        }
    }
    async fn terminate_session(&mut self, reason: SessTermReasonCode) -> (ReceiveState, SendState) {
        if let Err(err) = TcpClPacket::SessTerm(SessTermData {
            flags: SessTermFlags::empty(),
            reason,
        })
        .write(&mut self.writer)
        .await
        {
            error!("error while sending session terminate: {:?}", err);
        }
        (ReceiveState::Terminated, SendState::Terminated)
    }
    async fn process_bundle(&mut self, vec: Vec<u8>, tid: u64) -> anyhow::Result<ReceiveState> {
        match Bundle::try_from(vec) {
            Ok(bundle) => {
                tokio::spawn(async move {
                    if let Err(err) = crate::core::processing::receive(bundle).await {
                        error!("Failed to process bundle: {}", err);
                    }
                });
                Ok(ReceiveState::Idle)
            }
            Err(err) => {
                error!("Failed to parse bundle: {}", err);
                //error!("Failed bytes: {}", bp7::helpers::hexify(&vec));
                TcpClPacket::XferRefuse(XferRefuseData {
                    reason: XferRefuseReasonCode::NotAcceptable,
                    tid,
                })
                .write(&mut self.writer)
                .await?;
                Ok(ReceiveState::Idle)
            }
        }
    }
    /// Receive a new packet.
    /// Returns once transfer is finished and session is idle again.
    /// Result indicates whether connection is closed (true).
    async fn receive(
        &mut self,
        packet: TcpClPacket,
        (receive_state, send_state): (ReceiveState, SendState),
    ) -> anyhow::Result<(ReceiveState, SendState)> {
        match &packet {
            // session is terminated, send ack and return with true
            TcpClPacket::SessTerm(data) => {
                trace!("Received SessTerm: {:?}", data);
                if !data.flags.contains(SessTermFlags::REPLY) {
                    TcpClPacket::SessTerm(SessTermData {
                        flags: SessTermFlags::REPLY,
                        reason: data.reason,
                    })
                    .write(&mut self.writer)
                    .await?;
                }
                Ok((ReceiveState::Terminated, SendState::Terminated))
            }
            // receive a bundle
            TcpClPacket::XferSeg(data) => {
                debug!(
                    "Received XferSeg: TID={} LEN={} FLAGS={:?}",
                    data.tid, data.len, data.flags
                );
                match receive_state {
                    ReceiveState::Receiving(mut buffer, tid) => {
                        // transfer already started
                        if data.flags.contains(XferSegmentFlags::START) {
                            return Err(TcpSessionError::Protocol(packet).into());
                        }

                        if tid != data.tid {
                            return Err(TcpSessionError::Protocol(packet).into());
                        }

                        buffer.append(&mut data.buf.to_vec());
                        trace!("Sending XferAck: TID={}", data.tid);
                        TcpClPacket::XferAck(XferAckData {
                            tid: data.tid,
                            len: buffer.len() as u64,
                            flags: XferSegmentFlags::empty(),
                        })
                        .write(&mut self.writer)
                        .await?;

                        if data.flags.contains(XferSegmentFlags::END) {
                            Ok((self.process_bundle(buffer, data.tid).await?, send_state))
                        } else {
                            Ok((ReceiveState::Receiving(buffer, data.tid), send_state))
                        }
                    }
                    ReceiveState::Idle => {
                        if (data.flags.contains(XferSegmentFlags::END)
                            && !data.flags.contains(XferSegmentFlags::START))
                            || data.flags.is_empty()
                        {
                            return Err(TcpSessionError::Protocol(packet).into());
                        }
                        if data.flags.contains(XferSegmentFlags::START)
                            && !data.extensions.is_empty()
                        {
                            for extension in &data.extensions {
                                if extension.item_type == TransferExtensionItemType::BundleID
                                    && self.refuse_existing_bundles
                                {
                                    if let Ok(bundle_id) =
                                        String::from_utf8(extension.data.to_vec())
                                    {
                                        debug!("transfer extension: bundle id: {}", bundle_id);
                                        if (*STORE.lock()).has_item(&bundle_id) {
                                            debug!("refusing bundle, already in store");
                                            TcpClPacket::XferRefuse(XferRefuseData {
                                                reason: XferRefuseReasonCode::NotAcceptable,
                                                tid: data.tid,
                                            })
                                            .write(&mut self.writer)
                                            .await?;
                                            return Ok((receive_state, send_state));
                                        }
                                    }
                                }
                            }
                        }
                        let vec = data.buf.to_vec();
                        trace!("Sending XferAck: TID={}", data.tid);
                        TcpClPacket::XferAck(XferAckData {
                            tid: data.tid,
                            len: data.len,
                            flags: XferSegmentFlags::empty(),
                        })
                        .write(&mut self.writer)
                        .await?;
                        if data.flags.contains(XferSegmentFlags::END) {
                            Ok((self.process_bundle(vec, data.tid).await?, send_state))
                        } else {
                            Ok((ReceiveState::Receiving(vec, data.tid), send_state))
                        }
                    }
                    _ => Err(TcpSessionError::Protocol(packet).into()),
                }
            }
            TcpClPacket::XferAck(ack_data) => match send_state {
                SendState::TransferRequest(data, response) => {
                    if ack_data.tid != self.last_tid {
                        return Err(TcpSessionError::Protocol(packet).into());
                    }
                    Ok((receive_state, self.send_bundle(data, response).await?))
                }
                SendState::Sending(len, response) => {
                    if ack_data.tid != self.last_tid {
                        return Err(TcpSessionError::Protocol(packet).into());
                    }
                    if ack_data.len < len {
                        Ok((receive_state, SendState::Sending(len, response)))
                    } else {
                        if let Err(err) = response.send(TransferResult::Successful) {
                            error!("Failed to send response: {:?}", err);
                            return Err(TcpSessionError::Protocol(packet).into());
                        }

                        Ok((receive_state, SendState::Idle))
                    }
                }
                _ => Err(TcpSessionError::Protocol(packet).into()),
            },
            TcpClPacket::XferRefuse(refuse_data) => match send_state {
                SendState::TransferRequest(_, response) => {
                    if refuse_data.tid != self.last_tid {
                        return Err(TcpSessionError::Protocol(packet).into());
                    }
                    debug!("Received refuse");
                    if response.send(TransferResult::Successful).is_err() {
                        error!("Failed to send response");
                        return Err(TcpSessionError::Protocol(packet).into());
                    }
                    Ok((receive_state, SendState::Idle))
                }
                SendState::Sending(_, response) => {
                    if refuse_data.tid != self.last_tid {
                        return Err(TcpSessionError::Protocol(packet).into());
                    }
                    if response.send(TransferResult::Failure).is_err() {
                        error!("Failed to send response");
                        return Err(TcpSessionError::Protocol(packet).into());
                    }
                    Ok((receive_state, SendState::Idle))
                }
                _ => Err(TcpSessionError::Protocol(packet).into()),
            },
            _ => Err(TcpSessionError::Protocol(packet).into()),
        }
    }
    /// Send outgoing bundle.
    /// Result indicates whether connection is closed (true).
    async fn send(
        &mut self,
        data: (ByteBuffer, tokio::sync::oneshot::Sender<TransferResult>),
    ) -> anyhow::Result<SendState> {
        self.last_tid += 1;
        let (bndl_buf, tx_result) = data;

        if self.refuse_existing_bundles {
            let bundle = Bundle::try_from(bndl_buf.as_slice())?;
            let bundle_id = Bytes::copy_from_slice(bundle.id().as_bytes());
            // ask if peer already has bundle
            let extension = TransferExtensionItem {
                flags: TransferExtensionItemFlags::empty(),
                item_type: TransferExtensionItemType::BundleID,
                data: bundle_id,
            };
            let request_packet = TcpClPacket::XferSeg(XferSegData {
                flags: XferSegmentFlags::START,
                tid: self.last_tid,
                len: 0,
                buf: Bytes::new(),
                extensions: vec![extension],
            });
            request_packet.write(&mut self.writer).await?;
            Ok(SendState::TransferRequest(bndl_buf, tx_result))
        } else {
            self.send_bundle(bndl_buf, tx_result).await
        }
    }
    async fn send_bundle(
        &mut self,
        bndl_buf: ByteBuffer,
        tx_result: tokio::sync::oneshot::Sender<TransferResult>,
    ) -> anyhow::Result<SendState> {
        let now = Instant::now();
        let mut byte_vec = Vec::new();
        // split bundle data into chunks the size of remote maximum segment size
        for bytes in bndl_buf.chunks(self.remote_session_data.segment_mru as usize) {
            let buf = Bytes::copy_from_slice(bytes);
            let len = buf.len() as u64;
            //debug!("bytes len {}", len);
            let packet_data = XferSegData {
                flags: XferSegmentFlags::empty(),
                buf,
                len,
                tid: self.last_tid,
                extensions: Vec::new(),
            };
            byte_vec.push(packet_data);
        }
        if byte_vec.is_empty() {
            warn!("Emtpy bundle transfer, aborting");
            if tx_result.send(TransferResult::Failure).is_err() {
                error!("Failed to send response");
                bail!("Failed to send response");
            }
            return Ok(SendState::Idle);
        }
        // in this case start packet has already been sent
        if !self.refuse_existing_bundles {
            byte_vec
                .first_mut()
                .expect("no xfer segments, this should not be possible")
                .flags |= XferSegmentFlags::START;
        }
        byte_vec
            .last_mut()
            .expect("no xfer segments, this should not be possible")
            .flags |= XferSegmentFlags::END;
        // push packets to send task
        for packet in byte_vec {
            TcpClPacket::XferSeg(packet).write(&mut self.writer).await?;
        }
        info!(
            "Transmission time: {:?} for 1 bundles in {} bytes to {}",
            now.elapsed(),
            bndl_buf.len(),
            self.addr
        );
        Ok(SendState::Sending(bndl_buf.len() as u64, tx_result))
    }
}

impl TcpConnection {
    /// Session parameter negotiation
    async fn negotiate_session(&mut self) -> anyhow::Result<(SessInitData, SessInitData)> {
        let node_id = CONFIG.lock().host_eid.node_id().unwrap();
        let mut sess_init_data = SessInitData {
            keepalive: KEEPALIVE,
            segment_mru: SEGMENT_MRU,
            transfer_mru: TRANSFER_MRU,
            node_id,
        };

        let session_init = TcpClPacket::SessInit(sess_init_data.clone());
        session_init.write(&mut self.writer).await?;

        let response = TcpClPacket::read(&mut self.reader).await?;
        debug!("Received session parameters");
        if let TcpClPacket::SessInit(mut data) = response {
            let keepalive = sess_init_data.keepalive.min(data.keepalive);
            sess_init_data.keepalive = keepalive;
            data.keepalive = keepalive;
            Ok((sess_init_data, data))
        } else {
            Err(TcpClError::UnexpectedPacket.into())
        }
    }

    /// Initial contact header exchange
    async fn exchange_contact_header(&mut self) -> anyhow::Result<ContactHeaderFlags> {
        self.send_contact_header(ContactHeaderFlags::default())
            .await?;
        self.receive_contact_header().await
    }

    async fn send_contact_header(&mut self, flags: ContactHeaderFlags) -> anyhow::Result<()> {
        TcpClPacket::ContactHeader(flags)
            .write(&mut self.writer)
            .await?;
        Ok(())
    }

    async fn receive_contact_header(&mut self) -> anyhow::Result<ContactHeaderFlags> {
        let mut buf: [u8; 6] = [0; 6];
        self.reader.read_exact(&mut buf).await?;
        if &buf[0..4] != b"dtn!" {
            bail!("Invalid magic");
        }
        if buf[4] != 4 {
            bail!("Unsupported version");
        }
        Ok(ContactHeaderFlags::from_bits_truncate(buf[5]))
    }

    /// Establish a tcp session on this connection and insert it into a session list.
    async fn connect(
        mut self,
        rx_session_queue: mpsc::Receiver<(Vec<u8>, Sender<TransferResult>)>,
        active: bool,
    ) -> anyhow::Result<()> {
        // Phase 1
        debug!("Exchanging contact header, {}", self.addr);
        if let Err(err) = self.exchange_contact_header().await {
            bail!(
                "Failed to exchange contact header with {}: {}",
                self.addr,
                err
            );
        }
        // Phase 2
        debug!("Negotiating session parameters, {}", self.addr);
        match self.negotiate_session().await {
            Ok((local_parameters, remote_parameters)) => {
                // TODO: validate node id
                let remote_eid = EndpointID::try_from(remote_parameters.node_id.as_ref())
                    .expect("Invalid node id in tcpcl session");
                if !active && !peers_known(remote_eid.node().unwrap().as_ref()) {
                    let peer = DtnPeer::new(
                        remote_eid.clone(),
                        crate::PeerAddress::Ip(self.addr.ip()),
                        PeerType::Dynamic,
                        None,
                        vec![("tcp".into(), Some(self.addr.port()))],
                        HashMap::new(),
                    );
                    peers_add(peer);
                }

                info!(
                    "Started TCP session for {} @ {} | refuse existing bundles: {}",
                    remote_parameters.node_id, self.addr, self.refuse_existing_bundles
                );
                let session = TcpSession {
                    reader: self.reader,
                    writer: self.writer,
                    addr: self.addr,
                    refuse_existing_bundles: self.refuse_existing_bundles,
                    remote_session_data: remote_parameters,
                    _local_session_data: local_parameters,
                    last_tid: 0u64,
                    rx_session_queue,
                };
                session.run().await;
            }
            Err(err) => bail!("Failed to negotiate session for {}: {}", self.addr, err),
        }
        Ok(())
    }
}

pub struct Listener {
    tcp_listener: TcpListener,
    refuse_existing_bundles: bool,
}

impl Listener {
    async fn run(self) {
        loop {
            match self.tcp_listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Incoming connection from: {:?}", addr);
                    let (rx, tx) = stream.into_split();
                    let connection = TcpConnection {
                        reader: BufReader::new(rx),
                        writer: BufWriter::new(tx),
                        addr,
                        refuse_existing_bundles: self.refuse_existing_bundles,
                    };
                    // establish session and insert into shared session list
                    let (tx_session_queue, rx_session_queue) =
                        mpsc::channel::<(ByteBuffer, oneshot::Sender<TransferResult>)>(
                            INTERNAL_CHANNEL_BUFFER,
                        );
                    (*TCP_CONNECTIONS.lock().await).insert(addr, tx_session_queue);
                    tokio::spawn(async move {
                        if let Err(err) = connection.connect(rx_session_queue, false).await {
                            error!("Failed to establish TCP session with {}: {}", addr, err);
                        }
                    });
                }
                Err(e) => {
                    error!("Couldn't get client: {:?}", e)
                }
            }
        }
    }
}

async fn tcp_send_bundles(
    dest: String,
    bundle: ByteBuffer,
    refuse_existing_bundles: bool,
    reply: Sender<TransferResult>,
) -> anyhow::Result<()> {
    let addr: SocketAddr = dest.parse().unwrap();

    debug!("Locking connection for {}", addr);
    let (sender, receiver) = {
        let mut lock = TCP_CONNECTIONS.lock().await;

        if let Some(value) = lock.get(&addr) {
            if !value.is_closed() {
                (value.clone(), None)
            } else {
                lock.remove(&addr);
                let (tx_session_queue, rx_session_queue) =
                    mpsc::channel::<(ByteBuffer, oneshot::Sender<TransferResult>)>(
                        INTERNAL_CHANNEL_BUFFER,
                    );
                (*lock).insert(addr, tx_session_queue.clone());
                (tx_session_queue, Some(rx_session_queue))
            }
        } else {
            let (tx_session_queue, rx_session_queue) = mpsc::channel::<(
                ByteBuffer,
                oneshot::Sender<TransferResult>,
            )>(INTERNAL_CHANNEL_BUFFER);
            (*lock).insert(addr, tx_session_queue.clone());
            (tx_session_queue, Some(rx_session_queue))
        }
        // lock is dropped here
    };

    debug!("Connecting to {}", addr);
    // channel is inserted first into hashmap, even if connection is not yet established
    // connection is created here
    if let Some(rx_session_queue) = receiver {
        let conn_fut = TcpStream::connect(addr);
        match tokio::time::timeout(std::time::Duration::from_secs(3), conn_fut).await {
            Ok(Ok(stream)) => {
                let (rx, tx) = stream.into_split();
                let connection = TcpConnection {
                    reader: BufReader::new(rx),
                    writer: BufWriter::new(tx),
                    addr,
                    refuse_existing_bundles,
                };
                tokio::spawn(connection.connect(rx_session_queue, true));
            }
            Ok(Err(_)) => {
                if let Err(e) = reply.send(TransferResult::Failure) {
                    error!("Failed to send reply to internal sender channel: {:?}", e);
                }
                bail!("Couldn't connect to {}", addr);
            }
            Err(_) => {
                if let Err(e) = reply.send(TransferResult::Failure) {
                    error!("Failed to send reply to internal sender channel: {:?}", e);
                }
                bail!("Timeout connecting to {}", addr);
            }
        }
    }

    debug!("Sending bundle to {}", addr);
    // then push bundles to channel
    sender.send((bundle, reply)).await?;
    Ok(())
}

impl TcpConvergenceLayer {
    pub fn new(local_settings: Option<&HashMap<String, String>>) -> TcpConvergenceLayer {
        let local_addr: String = local_settings
            .and_then(|settings| settings.get("bind"))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());
        let port = local_settings
            .and_then(|settings| settings.get("port"))
            .and_then(|port_str| port_str.parse::<u16>().ok())
            .unwrap_or(4556);
        let local_refuse_existing_bundles = local_settings
            .and_then(|settings| settings.get("refuse-existing-bundles"))
            .and_then(|val| val.parse::<bool>().ok());
        let global_refuse_existing_bundles = CONFIG
            .lock()
            .cla_global_settings
            .get(&super::CLAsAvailable::TcpConvergenceLayer)
            .and_then(|settings| settings.get("refuse-existing-bundles"))
            .and_then(|ref_str| ref_str.parse::<bool>().ok())
            .unwrap_or(false);
        let refuse_existing_bundles =
            local_refuse_existing_bundles.unwrap_or(global_refuse_existing_bundles);
        debug!(
            "Extension settings: {:?}",
            CONFIG.lock().cla_global_settings
        );
        let (tx, mut rx) = mpsc::channel(INTERNAL_CHANNEL_BUFFER);

        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(remote, data, reply) => {
                        debug!(
                            "TcpConvergenceLayer: received transfer command for {}",
                            remote
                        );
                        tokio::spawn(async move {
                            if let Err(e) = tcp_send_bundles(
                                remote.clone(),
                                data,
                                refuse_existing_bundles,
                                reply,
                            )
                            .await
                            {
                                error!("Failed to send data to {}: {}", remote, e);
                            }
                        });
                    }
                    super::ClaCmd::Shutdown => {
                        debug!("TcpConvergenceLayer: received shutdown command");
                        break;
                    }
                }
            }
        });
        TcpConvergenceLayer {
            local_addr,
            local_port: port,
            refuse_existing_bundles,
            tx,
        }
    }
}

#[cla(tcp)]
#[derive(Debug)]
pub struct TcpConvergenceLayer {
    local_addr: String,
    local_port: u16,
    refuse_existing_bundles: bool,
    tx: mpsc::Sender<super::ClaCmd>,
}

#[async_trait]
impl ConvergenceLayerAgent for TcpConvergenceLayer {
    async fn setup(&mut self) {
        let tcp_listener = TcpListener::bind((self.local_addr.as_str(), self.local_port))
            .await
            .expect("Couldn't create TCP listener");
        let listener = Listener {
            tcp_listener,
            refuse_existing_bundles: self.refuse_existing_bundles,
        };
        tokio::spawn(listener.run());
    }

    fn port(&self) -> u16 {
        self.local_port
    }

    fn name(&self) -> &'static str {
        "tcp"
    }

    fn channel(&self) -> tokio::sync::mpsc::Sender<super::ClaCmd> {
        self.tx.clone()
    }
}

impl HelpStr for TcpConvergenceLayer {
    fn local_help_str() -> &'static str {
        "port=4556:refuse-existing-bundles=true|false:bind=0.0.0.0"
    }

    fn global_help_str() -> &'static str {
        "refuse-existing-bundles=true|false"
    }
}

impl std::fmt::Display for TcpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "tcp:{}:{}", self.local_addr, self.local_port)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::proto::XferSegData;
    use crate::cla::tcp::net::TcpClPacket;
    use crate::cla::tcp::proto::SessInitData;
    use crate::cla::tcp::proto::XferSegmentFlags;
    use anyhow::bail;
    use bytes::Bytes;
    use futures::executor::block_on;

    pub(crate) fn generate_xfer_segments(
        config: &SessInitData,
        buf: Bytes,
    ) -> anyhow::Result<Vec<XferSegData>> {
        static LAST_TRANSFER_ID: AtomicU64 = AtomicU64::new(0);
        // TODO: check for wrap around and SESS_TERM if overflow occurs
        let tid = LAST_TRANSFER_ID.fetch_add(1, Ordering::SeqCst);
        let mut segs = Vec::new();

        if buf.len() > config.transfer_mru as usize {
            bail!("bundle too big");
        }
        let fitting = u64::from(buf.len() as u64 % config.segment_mru != 0);
        let num_segs = (buf.len() as u64 / config.segment_mru) + fitting;

        for i in 0..num_segs {
            let mut flags = XferSegmentFlags::empty();
            if i == 0 {
                flags |= XferSegmentFlags::START;
            }
            if i == num_segs - 1 {
                flags |= XferSegmentFlags::END;
            }
            let len = if num_segs == 1 {
                // data fits in one segment
                buf.len() as u64
            } else if i == num_segs - 1 {
                // segment is the last one remaining
                buf.len() as u64 % config.segment_mru
            } else {
                // middle segment get filled to the max
                config.segment_mru
            };
            let base = (i * config.segment_mru) as usize;
            let seg = XferSegData {
                flags,
                tid,
                len,
                buf: buf.slice(base..base + len as usize),
                extensions: Vec::new(),
            };
            segs.push(seg);
        }

        Ok(segs)
    }

    fn perform_gen_xfer_segs_test(
        segment_mru: u64,
        transfer_mru: u64,
        data_len: u64,
    ) -> anyhow::Result<Vec<XferSegData>> {
        let config = SessInitData {
            keepalive: 0,
            segment_mru,
            transfer_mru,
            node_id: "node1".into(),
        };
        //        let data_raw: [u8; data_len] = [0; data_len];
        let data_raw: Vec<u8> = vec![0x90; data_len as usize];

        let fitting = u64::from(data_len % segment_mru != 0);
        let num_expected_segs = ((data_len / segment_mru) + fitting) as usize;

        //let data = Bytes::copy_from_slice(&data_raw);
        let data = Bytes::copy_from_slice(&data_raw);

        let segs =
            generate_xfer_segments(&config, data).expect("error generating xfer segment list");
        assert_eq!(segs.len(), num_expected_segs);

        assert!(segs[0].flags.contains(XferSegmentFlags::START));
        assert!(segs[num_expected_segs - 1]
            .flags
            .contains(XferSegmentFlags::END));

        Ok(segs)
    }
    #[test]
    fn gen_xfer_segs_single_seg() {
        let segs =
            perform_gen_xfer_segs_test(42, 100, 40).expect("error generating xfer segment list");
        dbg!(&segs);
        assert_eq!(segs.len(), 1);
    }

    #[test]
    fn gen_xfer_segs_two_segs() {
        let segs =
            perform_gen_xfer_segs_test(42, 100, 45).expect("error generating xfer segment list");
        dbg!(&segs);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn gen_xfer_segs_three_segs() {
        let segs =
            perform_gen_xfer_segs_test(10, 100, 28).expect("error generating xfer segment list");
        dbg!(&segs);
        assert_eq!(segs.len(), 3);
    }

    #[test]
    fn gen_xfer_segs_seg_edge_case() {
        let segs =
            perform_gen_xfer_segs_test(10, 100, 10).expect("error generating xfer segment list");
        dbg!(&segs);
        assert_eq!(segs.len(), 1);
    }

    #[test]
    #[should_panic]
    fn gen_xfer_segs_exceeding_t_mru() {
        perform_gen_xfer_segs_test(42, 100, 180).unwrap_err();
    }

    #[test]
    fn serialize_deserialize() {
        let segs =
            perform_gen_xfer_segs_test(10, 100, 10).expect("error generating xfer segment list");
        for s in segs {
            let mut buf = Vec::new();
            let packet = TcpClPacket::XferSeg(s);
            block_on(packet.write(&mut buf)).unwrap();
            let mut slice = buf.as_ref();
            let result = block_on(TcpClPacket::read(&mut slice)).unwrap();
            dbg!(&packet);
            dbg!(&buf);
            dbg!(&result);
            assert!(packet == result);
        }
    }
}
