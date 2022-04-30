mod buffer_flush;
pub mod net;
pub mod proto;

use self::net::*;

use super::tcp::proto::*;
use super::{ClaCmd, ConvergenceLayerAgent, HelpStr, TransferResult};
use crate::core::store::BundleStore;
use crate::core::PeerType;
use crate::{peers_add, peers_known, STORE};
use crate::{DtnPeer, CONFIG};
use anyhow::bail;
use async_trait::async_trait;
use bp7::{Bundle, EndpointID};
use buffer_flush::StreamCustomExt;
use bytes::Bytes;
use dtn7_codegen::cla;
use futures::stream::{self, once, SplitSink, SplitStream};
use futures::{future, SinkExt, StreamExt, TryStreamExt};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::mem;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::io;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::oneshot::{self, Sender};
use tokio::time::{Duration, Instant};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::codec::Framed;

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

type SessionMap = HashMap<SocketAddr, mpsc::Sender<(Bytes, oneshot::Sender<TransferResult>)>>;

const KEEPALIVE: u16 = 10;
const SEGMENT_MRU: u64 = 64000;
const TRANSFER_MRU: u64 = 64000;
// 2 provides some level of concurrency without buffering too many elements
// TODO adjust channel sizes if pipelining/parallel processing is fixed
const INTERNAL_CHANNEL_BUFFER: usize = 2;
const PACKET_BUFFER: usize = 15;

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
    #[error("IO error in TCP stream")]
    Stream(io::Error),
    #[error("IO error in TCP sink")]
    Sink(io::Error),
    #[error("Bundle parsing error")]
    Parsing(#[from] bp7::error::Error),
    #[error("Session terminated")]
    Terminated,
}

impl From<TransferResult> for TcpSessionError {
    fn from(_: TransferResult) -> Self {
        TcpSessionError::ResultChannel
    }
}

/// Initial tcp connection.
/// Session not yet established.
struct TcpConnection {
    reader: SplitStream<Framed<TcpStream, TcpClCodec>>,
    writer: SplitSink<Framed<TcpStream, TcpClCodec>, TcpClPacket>,
    addr: SocketAddr,
    refuse_existing_bundles: bool,
}

struct TcpSession {
    writer: SplitSink<Framed<TcpStream, TcpClCodec>, TcpClPacket>,
    reader: SplitStream<Framed<TcpStream, TcpClCodec>>,
    addr: SocketAddr,
    refuse_existing_bundles: bool,
    remote_session_data: SessInitData,
    _local_session_data: SessInitData,
    rx_session_queue: mpsc::Receiver<(Bytes, Sender<TransferResult>)>,
}

#[derive(Debug)]
enum ReceiveState {
    Idle,
    Receiving(Vec<u8>, u64),
}

#[derive(Debug)]
struct Transfer {
    response: Sender<TransferResult>,
    state: TransferState,
}

#[derive(Debug)]
enum TransferState {
    Request(Bytes),
    Pending(u64),
}

#[derive(Debug)]
enum Items {
    TcpIncoming(TcpClPacket),
    BundleSend(Bytes, Sender<TransferResult>),
    Terminated,
    ReadTimeout,
}

struct State {
    transfers: HashMap<u64, Transfer>,
    last_tid: u64,
    addr: SocketAddr,
    refuse_existing_bundles: bool,
    remote_session_data: SessInitData,
    receive_state: ReceiveState,
    terminated: bool,
}

impl TcpSession {
    async fn run(self) {
        // map tcp rx, bundle queue into combined stream
        let reader_mapped = tokio_stream::StreamExt::timeout(
            self.reader
                .map_err(TcpSessionError::Stream)
                .map_ok(Items::TcpIncoming)
                .chain(once(future::ready(Ok(Items::Terminated)))),
            Duration::from_secs(self.remote_session_data.keepalive as u64 * 2),
        )
        .map(|value| match value {
            Ok(it) => it,
            Err(_) => Ok(Items::ReadTimeout),
        });
        let queue_mapped = ReceiverStream::new(self.rx_session_queue)
            .map(|(bundle, response)| Ok(Items::BundleSend(bundle, response)))
            .chain(once(future::ready(Ok(Items::Terminated))));
        let process = tokio_stream::StreamExt::merge(reader_mapped, queue_mapped);
        let state = State {
            transfers: HashMap::new(),
            addr: self.addr,
            refuse_existing_bundles: self.refuse_existing_bundles,
            last_tid: 0,
            remote_session_data: self.remote_session_data.clone(),
            receive_state: ReceiveState::Idle,
            terminated: false,
        };

        // process values
        // scan = stream version of fold, process commands, return vector of packets to be sent
        // try_flatten = combine stream into current stream
        // forward requires TryStream with same error type as sink, send packets
        if let Err(err) = tokio_stream::StreamExt::timeout(
            process
                .scan(state, |state, item| {
                    future::ready(match item {
                        Ok(item) => Some(Self::process(state, item).map(|packets| {
                            if state.terminated {
                                stream::iter(packets)
                                    .map(Result::Ok)
                                    .chain(once(future::ready(Result::Err(
                                        TcpSessionError::Terminated,
                                    ))))
                                    .left_stream()
                            } else {
                                stream::iter(packets).map(Result::Ok).right_stream()
                            }
                        })),
                        Err(err) => Some(Err(err)),
                    })
                })
                .try_flatten(),
            Duration::from_secs(self.remote_session_data.keepalive as u64),
        )
        .map(|value| match value {
            Ok(it) => it,
            Err(_) => Ok(TcpClPacket::KeepAlive),
        })
        .forward_flush(
            self.writer.sink_map_err(TcpSessionError::Sink),
            PACKET_BUFFER,
        )
        //.forward(self.writer.sink_map_err(TcpSessionError::Sink))
        .await
        {
            error!("Tcp Session failed: {}", err)
        }

        info!("Tcp Session for {} ended", self.addr);
    }
    fn process_bundle(vec: Vec<u8>) {
        tokio::spawn(async move {
            match Bundle::try_from(vec) {
                Ok(bundle) => {
                    if let Err(err) = crate::core::processing::receive(bundle).await {
                        error!("Failed to process bundle: {}", err);
                    }
                }
                Err(err) => {
                    error!("Failed to parse bundle: {}", err);
                    //error!("Failed bytes: {}", bp7::helpers::hexify(&vec));
                }
            }
        });
    }
    fn process(state: &mut State, packet: Items) -> Result<Vec<TcpClPacket>, TcpSessionError> {
        match packet {
            Items::TcpIncoming(packet) => Self::receive(state, packet),
            Items::BundleSend(bundle, response) => Self::send(state, bundle, response),
            Items::Terminated => {
                state.terminated = true;
                Ok(vec![])
            }
            Items::ReadTimeout => {
                state.terminated = true;
                debug!(
                    "Terminate session for {} because of idle timeout",
                    state.addr
                );
                Ok(vec![TcpClPacket::SessTerm(SessTermData {
                    flags: SessTermFlags::empty(),
                    reason: SessTermReasonCode::IdleTimeout,
                })])
            }
        }
    }
    fn receive(
        state: &mut State,
        packet: TcpClPacket,
    ) -> Result<Vec<TcpClPacket>, TcpSessionError> {
        let now = Instant::now();
        match packet {
            // session is terminated, send ack and return with true
            TcpClPacket::SessTerm(data) => {
                trace!("Received SessTerm: {:?}", data);
                state.terminated = true;
                if !data.flags.contains(SessTermFlags::REPLY) {
                    return Ok(vec![TcpClPacket::SessTerm(SessTermData {
                        flags: SessTermFlags::REPLY,
                        reason: data.reason,
                    })]);
                }
            }
            // receive a bundle
            TcpClPacket::XferSeg(data) => {
                debug!(
                    "Received XferSeg: TID={} LEN={} FLAGS={:?}",
                    data.tid, data.len, data.flags
                );
                match mem::replace(&mut state.receive_state, ReceiveState::Idle) {
                    ReceiveState::Receiving(mut buffer, tid) => {
                        // transfer already started
                        if data.flags.contains(XferSegmentFlags::START) {
                            return Err(TcpSessionError::Protocol(TcpClPacket::XferSeg(data)));
                        }

                        if tid != data.tid {
                            return Err(TcpSessionError::Protocol(TcpClPacket::XferSeg(data)));
                        }

                        buffer.append(&mut data.buf.to_vec());
                        trace!("Sending XferAck: TID={}", data.tid);
                        let response_packets = vec![TcpClPacket::XferAck(XferAckData {
                            tid: data.tid,
                            len: buffer.len() as u64,
                            flags: data.flags,
                        })];

                        if data.flags.contains(XferSegmentFlags::END) {
                            Self::process_bundle(buffer);
                            state.receive_state = ReceiveState::Idle;
                        } else {
                            state.receive_state = ReceiveState::Receiving(buffer, data.tid);
                        }
                        return Ok(response_packets);
                    }
                    ReceiveState::Idle => {
                        if (data.flags.contains(XferSegmentFlags::END)
                            && !data.flags.contains(XferSegmentFlags::START))
                            || data.flags.is_empty()
                        {
                            return Err(TcpSessionError::Protocol(TcpClPacket::XferSeg(data)));
                        }
                        let vec = data.buf.to_vec();
                        trace!("Sending XferAck: TID={}", data.tid);
                        let response_packets = vec![TcpClPacket::XferAck(XferAckData {
                            tid: data.tid,
                            len: data.len,
                            flags: data.flags,
                        })];
                        debug!("TIME RECEIVING IDLE: {:?}", now.elapsed());
                        if data.flags.contains(XferSegmentFlags::END) {
                            Self::process_bundle(vec);
                            state.receive_state = ReceiveState::Idle;
                        } else {
                            state.receive_state = ReceiveState::Receiving(vec, data.tid);
                        }
                        return Ok(response_packets);
                    }
                }
            }
            TcpClPacket::XferAck(ack_data) => {
                let transfer = state.transfers.remove(&ack_data.tid);
                match transfer {
                    Some(transfer) => match transfer.state {
                        TransferState::Request(_) => return Err(TcpSessionError::Protocol(packet)),
                        TransferState::Pending(len) => {
                            if ack_data.len == len {
                                if let Err(err) = transfer.response.send(TransferResult::Successful)
                                {
                                    error!("Failed to send response: {:?}", err);
                                    return Err(TcpSessionError::ResultChannel);
                                }
                            } else {
                                state.transfers.insert(ack_data.tid, transfer);
                            }
                        }
                    },
                    None => return Err(TcpSessionError::Protocol(packet)),
                }
            }
            TcpClPacket::XferRefuse(refuse_data) => {
                state.transfers.remove(&refuse_data.tid);
                warn!("Transfer {} refused", refuse_data.tid);
            }
            TcpClPacket::KeepAlive => { // do nothing
            }
            TcpClPacket::BundleIDRequest(data) => {
                if state.refuse_existing_bundles {
                    if let Ok(bundle_id) = String::from_utf8(data.data.to_vec()) {
                        debug!("session extension: bundle id: {}", bundle_id);
                        if (*STORE.lock()).has_item(&bundle_id) {
                            debug!("refusing bundle, already in store");
                            return Ok(vec![TcpClPacket::BundleIDResponse(BundleIDResponseData {
                                tid: data.tid,
                                code: BundleIDResponse::Refuse,
                            })]);
                        } else {
                            debug!("accepting bundle");
                            return Ok(vec![TcpClPacket::BundleIDResponse(BundleIDResponseData {
                                tid: data.tid,
                                code: BundleIDResponse::Accept,
                            })]);
                        }
                    }
                } else {
                    return Err(TcpSessionError::Protocol(TcpClPacket::BundleIDRequest(
                        data,
                    )));
                }
            }
            TcpClPacket::BundleIDResponse(data) => {
                if state.refuse_existing_bundles {
                    let transfer = state.transfers.remove(&data.tid);
                    match transfer {
                        Some(transfer) => match data.code {
                            BundleIDResponse::Accept => match transfer.state {
                                TransferState::Request(bytes) => {
                                    return Self::send_bundle(state, bytes, transfer.response)
                                }
                                TransferState::Pending(_) => {
                                    return Err(TcpSessionError::Protocol(
                                        TcpClPacket::BundleIDResponse(data),
                                    ))
                                }
                            },
                            BundleIDResponse::Refuse => {
                                debug!("Received refuse");
                                if let Err(err) = transfer.response.send(TransferResult::Successful)
                                {
                                    error!("Failed to send response: {:?}", err);
                                    return Err(TcpSessionError::ResultChannel);
                                }
                            }
                        },
                        None => {
                            return Err(TcpSessionError::Protocol(TcpClPacket::BundleIDResponse(
                                data,
                            )))
                        }
                    }
                } else {
                    return Err(TcpSessionError::Protocol(TcpClPacket::BundleIDResponse(
                        data,
                    )));
                }
            }
            _ => return Err(TcpSessionError::Protocol(packet)),
        }
        Ok(vec![])
    }
    fn send(
        state: &mut State,
        bndl_buf: Bytes,
        tx_result: tokio::sync::oneshot::Sender<TransferResult>,
    ) -> Result<Vec<TcpClPacket>, TcpSessionError> {
        debug!("Beginning new bundle transfer for {}", state.addr);
        state.last_tid += 1;

        if state.refuse_existing_bundles {
            let bundle = Bundle::try_from(bndl_buf.as_ref())?;
            let bundle_id = Bytes::copy_from_slice(bundle.id().as_bytes());
            // ask if peer already has bundle
            let request_packet = TcpClPacket::BundleIDRequest(BundleIDRequestData {
                tid: state.last_tid,
                data: bundle_id,
            });
            state.transfers.insert(
                state.last_tid,
                Transfer {
                    response: tx_result,
                    state: TransferState::Request(bndl_buf),
                },
            );
            Ok(vec![request_packet])
        } else {
            Self::send_bundle(state, bndl_buf, tx_result)
        }
    }
    fn send_bundle(
        state: &mut State,
        mut bndl_buf: Bytes,
        tx_result: tokio::sync::oneshot::Sender<TransferResult>,
    ) -> Result<Vec<TcpClPacket>, TcpSessionError> {
        let now = Instant::now();
        let bndl_len = bndl_buf.len();
        // split bundle data into chunks the size of remote maximum segment size
        let mut flags = XferSegmentFlags::START;
        let mut packets = Vec::new();
        while bndl_buf.len() > state.remote_session_data.segment_mru as usize {
            let data = bndl_buf.split_to(state.remote_session_data.segment_mru as usize + 1);
            let len = data.len();
            let packet_data = XferSegData {
                flags,
                buf: data,
                len: len as u64,
                tid: state.last_tid,
                extensions: Vec::new(),
            };
            packets.push(TcpClPacket::XferSeg(packet_data));
            flags = XferSegmentFlags::empty();
        }
        let len = bndl_buf.len();
        let packet_data = XferSegData {
            flags: flags | XferSegmentFlags::END,
            buf: bndl_buf,
            len: len as u64,
            tid: state.last_tid,
            extensions: Vec::new(),
        };
        packets.push(TcpClPacket::XferSeg(packet_data));

        info!(
            "Transmission time: {:?} for 1 bundles in {} bytes to {}",
            now.elapsed(),
            bndl_len,
            state.addr
        );
        state.transfers.insert(
            state.last_tid,
            Transfer {
                response: tx_result,
                state: TransferState::Pending(bndl_len as u64),
            },
        );
        Ok(packets)
    }
}

impl TcpConnection {
    /// Session parameter negotiation
    async fn negotiate_session(&mut self) -> anyhow::Result<(SessInitData, SessInitData)> {
        let node_id = (*CONFIG.lock()).host_eid.node_id().unwrap();
        let extensions = if self.refuse_existing_bundles {
            vec![SessionExtensionItem {
                flags: SessionExtensionItemFlags::empty(),
                item_type: SessionExtensionItemType::BundleID,
                data: "!bundleid".into(),
            }]
        } else {
            vec![]
        };
        let mut sess_init_data = SessInitData {
            keepalive: KEEPALIVE,
            segment_mru: SEGMENT_MRU,
            transfer_mru: TRANSFER_MRU,
            node_id,
            extensions,
        };

        let session_init = TcpClPacket::SessInit(sess_init_data.clone());
        self.writer.send(session_init).await?;
        self.writer.flush().await?;

        let response = self.reader.next().await.unwrap()?;
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
        self.writer.send(TcpClPacket::ContactHeader(flags)).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn receive_contact_header(&mut self) -> anyhow::Result<ContactHeaderFlags> {
        let response = self.reader.next().await.unwrap()?;
        if let TcpClPacket::ContactHeader(flags) = response {
            Ok(flags)
        } else {
            Err(TcpClError::UnexpectedPacket.into())
        }
    }

    /// Establish a tcp session on this connection and insert it into a session list.
    async fn connect(
        mut self,
        rx_session_queue: mpsc::Receiver<(Bytes, Sender<TransferResult>)>,
        active: bool,
    ) -> anyhow::Result<()> {
        let now = Instant::now();
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
                let mut refuse_supported = false;
                for ext in &remote_parameters.extensions {
                    refuse_supported |= matches!(ext.item_type, SessionExtensionItemType::BundleID)
                        && ext.data == "!bundleid";
                }
                let session = TcpSession {
                    writer: self.writer,
                    reader: self.reader,
                    addr: self.addr,
                    refuse_existing_bundles: self.refuse_existing_bundles && refuse_supported,
                    remote_session_data: remote_parameters,
                    _local_session_data: local_parameters,
                    rx_session_queue,
                };
                debug!("TIME SESSION CONNECT {:?}", now.elapsed());
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
                    let framed = Framed::new(stream, TcpClCodec { startup: true });
                    info!("Incoming connection from: {:?}", addr);
                    let (tx, rx) = framed.split();
                    let connection = TcpConnection {
                        reader: rx,
                        writer: tx,
                        addr,
                        refuse_existing_bundles: self.refuse_existing_bundles,
                    };
                    // establish session and insert into shared session list
                    let (tx_session_queue, rx_session_queue) =
                        mpsc::channel::<(Bytes, oneshot::Sender<TransferResult>)>(2);
                    (*TCP_CONNECTIONS.lock()).insert(addr, tx_session_queue);
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
    bundle: Bytes,
    refuse_existing_bundles: bool,
    reply: Sender<TransferResult>,
) -> anyhow::Result<()> {
    let now = Instant::now();
    let addr: SocketAddr = dest.parse().unwrap();

    let (sender, receiver) = {
        let mut lock = TCP_CONNECTIONS.lock();
        if let Some(value) = lock.get(&addr) {
            trace!("Found existing connection");
            if !value.is_closed() {
                (value.clone(), None)
            } else {
                trace!("Existing connection is closed");
                lock.remove(&addr);
                let (tx_session_queue, rx_session_queue) =
                    mpsc::channel::<(Bytes, oneshot::Sender<TransferResult>)>(2);
                (*lock).insert(addr, tx_session_queue.clone());
                (tx_session_queue, Some(rx_session_queue))
            }
        } else {
            let (tx_session_queue, rx_session_queue) =
                mpsc::channel::<(Bytes, oneshot::Sender<TransferResult>)>(2);
            (*lock).insert(addr, tx_session_queue.clone());
            (tx_session_queue, Some(rx_session_queue))
        }
        // lock is dropped here
    };

    // channel is inserted first into hashmap, even if connection is not yet established
    // connection is created here
    if let Some(rx_session_queue) = receiver {
        trace!("Starting new connection to {}", addr);
        let conn_fut = TcpStream::connect(addr);
        match tokio::time::timeout(std::time::Duration::from_secs(3), conn_fut).await {
            Ok(Ok(stream)) => {
                let framed = Framed::new(stream, TcpClCodec { startup: true });
                let (tx, rx) = framed.split();
                let connection = TcpConnection {
                    reader: rx,
                    writer: tx,
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
    debug!("tcp_send_bundles channel capacity: {}", sender.capacity());
    // then push bundles to channel
    sender.send((bundle, reply)).await?;
    debug!("TIME tcp_send_bundles: {:?}", now.elapsed());
    Ok(())
}

impl TcpConvergenceLayer {
    pub fn new(local_settings: Option<&HashMap<String, String>>) -> TcpConvergenceLayer {
        let port = local_settings
            .and_then(|settings| settings.get("port"))
            .and_then(|port_str| port_str.parse::<u16>().ok())
            .unwrap_or(4556);
        let local_refuse_existing_bundles = local_settings
            .and_then(|settings| settings.get("refuse-existing-bundles"))
            .and_then(|val| val.parse::<bool>().ok());
        let global_refuse_existing_bundles = (*CONFIG.lock())
            .cla_global_settings
            .get(&super::CLAsAvailable::TcpConvergenceLayer)
            .and_then(|settings| settings.get("refuse-existing-bundles"))
            .and_then(|ref_str| ref_str.parse::<bool>().ok())
            .unwrap_or(false);
        let refuse_existing_bundles =
            local_refuse_existing_bundles.unwrap_or(global_refuse_existing_bundles);
        debug!(
            "Extension settings: {:?}",
            (*CONFIG.lock()).cla_global_settings
        );
        let (tx, rx) = mpsc::channel(INTERNAL_CHANNEL_BUFFER);
        let receiver_stream = ReceiverStream::new(rx);

        TcpConvergenceLayer {
            local_port: port,
            refuse_existing_bundles,
            tx,
            receiver_stream: Some(receiver_stream),
        }
    }
}

#[cla(tcp)]
#[derive(Debug)]
pub struct TcpConvergenceLayer {
    local_port: u16,
    refuse_existing_bundles: bool,
    tx: mpsc::Sender<super::ClaCmd>,
    receiver_stream: Option<ReceiverStream<ClaCmd>>,
}

#[async_trait]
impl ConvergenceLayerAgent for TcpConvergenceLayer {
    async fn setup(&mut self) {
        let tcp_listener = TcpListener::bind(("0.0.0.0", self.local_port))
            .await
            .expect("Couldn't create TCP listener");
        let listener = Listener {
            tcp_listener,
            refuse_existing_bundles: self.refuse_existing_bundles,
        };
        tokio::spawn(listener.run());
        let refuse_existing_bundles = self.refuse_existing_bundles;
        let receiver_stream = self.receiver_stream.take().unwrap();
        tokio::spawn(
            receiver_stream
                .take_while(|item| future::ready(!matches!(item, super::ClaCmd::Shutdown)))
                .for_each_concurrent(10, move |cmd| async move {
                    match cmd {
                        super::ClaCmd::Transfer(remote, data, reply) => {
                            debug!(
                                "TcpConvergenceLayer: received transfer command for {}",
                                remote
                            );

                            if let Err(e) = tcp_send_bundles(
                                remote.clone(),
                                Bytes::from(data),
                                refuse_existing_bundles,
                                reply,
                            )
                            .await
                            {
                                error!("Failed to send data to {}: {}", remote, e);
                            }
                        }
                        super::ClaCmd::Shutdown => unreachable!(),
                    }
                }),
        );
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

impl Clone for TcpConvergenceLayer {
    fn clone(&self) -> Self {
        Self {
            local_port: self.local_port.clone(),
            refuse_existing_bundles: self.refuse_existing_bundles.clone(),
            tx: self.tx.clone(),
            receiver_stream: None,
        }
    }
}

impl HelpStr for TcpConvergenceLayer {
    fn local_help_str() -> &'static str {
        "port=4556:refuse-existing-bundles=true|false"
    }

    fn global_help_str() -> &'static str {
        "refuse-existing-bundles=true|false"
    }
}

impl std::fmt::Display for TcpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "tcp")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::proto::XferSegData;
    use super::{TcpConvergenceLayer, INTERNAL_CHANNEL_BUFFER};
    use crate::cla::tcp::proto::SessInitData;
    use crate::cla::tcp::proto::XferSegmentFlags;
    use crate::cla::{ClaCmd, ConvergenceLayerAgent, TransferResult};
    use anyhow::bail;
    use bytes::Bytes;
    use tokio::sync::oneshot::{self, Receiver, Sender};
    use tokio::time::Instant;

    #[tokio::test]
    async fn pipelining() {
        std::env::set_var("RUST_LOG", "dtn7=trace,dtnd=trace");
        pretty_env_logger::init();
        let mut a = TcpConvergenceLayer::new(None);
        let mut map = HashMap::new();
        map.insert("port".to_string(), "4557".to_string());
        let mut b = TcpConvergenceLayer::new(Some(&map));
        let a_channel = a.channel();

        //send three bundles from a to b
        let mut responses = Vec::new();
        for (data, remote, sender, receiver) in generate_bundles(INTERNAL_CHANNEL_BUFFER) {
            responses.push(async move { receiver.await.unwrap() });
            a_channel
                .send(ClaCmd::Transfer(remote, data, sender))
                .await
                .unwrap();
        }

        let now = Instant::now();
        b.setup().await;
        a.setup().await;
        use futures::future::join_all;
        join_all(responses).await;
        println!("Elapsed {:?}", now.elapsed());
    }

    fn generate_bundles(
        num: usize,
    ) -> Vec<(
        Vec<u8>,
        String,
        Sender<TransferResult>,
        Receiver<TransferResult>,
    )> {
        let mut vec = Vec::new();
        for _ in 0..num {
            let data_raw: Vec<u8> = vec![0x90; 100];
            let remote = "0.0.0.0:4557".to_string();
            let (sender, receiver) = oneshot::channel::<TransferResult>();
            vec.push((data_raw, remote, sender, receiver));
        }
        vec
    }

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
        let fitting = if buf.len() as u64 % config.segment_mru == 0 {
            0
        } else {
            1
        };
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
            extensions: Vec::new(),
        };
        //        let data_raw: [u8; data_len] = [0; data_len];
        let data_raw: Vec<u8> = vec![0x90; data_len as usize];

        let fitting = if data_len % segment_mru == 0 { 0 } else { 1 };
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
}
