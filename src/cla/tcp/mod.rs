pub mod net;
pub mod proto;

use self::net::*;

use super::ConvergenceLayerAgent;
use async_trait::async_trait;
use bp7::{Bundle, ByteBuffer};
use futures::lock::Mutex;
//use futures_util::stream::StreamExt;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::sync::Arc;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time;
//use std::net::TcpStream;
use super::tcp::proto::*;
use crate::CONFIG;
use anyhow::anyhow;
use anyhow::bail;
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

const KEEPALIVE: u16 = 30;
const SEGMENT_MRU: u64 = 64000;
const TRANSFER_MRU: u64 = 64000;
const INTERNAL_CHANNEL_BUFFER: usize = 50;

/// Represents a tcp convergence layer session.
/// Convergence layer connection is established at this point.
struct TcpClSession {
    /// Transmitter to tcp sending task
    tx_session_outgoing: mpsc::Sender<TcpClPacket>,
    /// Receiver from tcp receiving task
    rx_session_incoming: mpsc::Receiver<TcpClPacket>,
    /// Queue of all outgoing packages
    rx_session_queue: mpsc::Receiver<(ByteBuffer, oneshot::Sender<bool>)>,
    /// Local session parameters
    data_local: SessInitData,
    /// Remote session parameters
    data_remote: SessInitData,
    /// Last transaction id, incremented by 1
    last_tid: u64,
}

impl TcpClSession {
    /// Run this future until the connection is closed
    async fn run(mut self) {
        loop {
            // session is idle, try to send/receive
            tokio::select! {
                // poll for new incoming packages
                received = self.rx_session_incoming.recv() => {
                    if let Some(packet) = received {
                        match self.receive(packet).await {
                            Ok(stop) => if stop { break; },
                            Err(err) => {
                                error!("error while receiving: {:?}", err);
                                break;
                            },
                        }
                    } else {
                        break;
                    }
                },
                // send outgoing packages
                bundle = self.rx_session_queue.recv() =>  {
                    if let Some(message) = bundle {
                        debug!("Sending bundle");
                        self.send(message).await;
                    } else {
                        break;
                    }
                }
            }
        }
    }
    /// Receive a new packet.
    /// Returns once transfer is finished and session is idle again.
    /// Result indicates whether connection is closed (true).
    async fn receive(&mut self, packet: TcpClPacket) -> anyhow::Result<bool> {
        match packet {
            // session is terminated, send ack and return with true
            TcpClPacket::SessTerm(data) => {
                if !data.flags.contains(SessTermFlags::REPLY) {
                    self.tx_session_outgoing
                        .send(TcpClPacket::SessTerm(SessTermData {
                            flags: SessTermFlags::REPLY,
                            reason: data.reason,
                        }))
                        .await?;
                }
                Ok(true)
            }
            // receive a bundle
            TcpClPacket::XferSeg(data) => {
                if data.flags.contains(XferSegmentFlags::END)
                    && !data.flags.contains(XferSegmentFlags::START)
                {
                    return Err(anyhow!(
                        "Received XferSeg with end flag, unexpected in idle mode"
                    ));
                }
                if data.flags.is_empty() {
                    return Err(anyhow!(
                        "Received XferSeg with no flags, unexpected in idle mode"
                    ));
                }
                let mut vec = data.buf.to_vec();
                self.tx_session_outgoing
                    .send(TcpClPacket::XferAck(XferAckData {
                        tid: data.tid,
                        len: data.len,
                        flags: XferSegmentFlags::empty(),
                    }))
                    .await?;
                let mut connection_closing = false;
                // receive further packages until transfer is finished
                if !data.flags.contains(XferSegmentFlags::END) {
                    let mut len = data.len;
                    if data.len > self.data_local.segment_mru {
                        self.tx_session_outgoing
                            .send(TcpClPacket::XferRefuse(XferRefuseData {
                                reason: XferRefuseReasonCode::NotAcceptable,
                                tid: data.tid,
                            }))
                            .await?;
                    }
                    loop {
                        if let Some(packet) = self.rx_session_incoming.recv().await {
                            match packet {
                                TcpClPacket::SessTerm(mut data) => {
                                    connection_closing = true;
                                    data.flags |= SessTermFlags::REPLY;
                                    self.tx_session_outgoing
                                        .send(TcpClPacket::SessTerm(data))
                                        .await?;
                                }
                                TcpClPacket::XferSeg(data) => {
                                    vec.append(&mut data.buf.to_vec());
                                    len += data.len;
                                    self.tx_session_outgoing
                                        .send(TcpClPacket::XferAck(XferAckData {
                                            tid: data.tid,
                                            len,
                                            flags: XferSegmentFlags::empty(),
                                        }))
                                        .await?;
                                    if data.flags.contains(XferSegmentFlags::END) {
                                        break;
                                    }
                                }
                                TcpClPacket::MsgReject(data) => {
                                    warn!("Received message reject: {:?}", data);
                                }
                                _ => {
                                    return Err(anyhow!("Unexpected packet, {:?}", packet));
                                }
                            }
                        } else {
                            return Err(anyhow!("Connection closed while receiving transfer"));
                        }
                    }
                }
                debug!("Parsing bundle from received tcp bytes");
                // parse bundle
                match Bundle::try_from(vec) {
                    Ok(bundle) => {
                        tokio::spawn(async move {
                            if let Err(err) = crate::core::processing::receive(bundle).await {
                                error!("Failed to process bundle: {}", err);
                            }
                        });
                    }
                    Err(err) => {
                        error!("Failed to parse bundle: {}", err);
                        self.tx_session_outgoing
                            .send(TcpClPacket::XferRefuse(XferRefuseData {
                                reason: XferRefuseReasonCode::NotAcceptable,
                                tid: data.tid,
                            }))
                            .await?;
                    }
                }
                Ok(connection_closing)
            }
            _ => Err(anyhow!("Unexpected packet, {:?}", packet)),
        }
    }
    /// Send outgoing bundle.
    /// Result indicates whether connection is closed (true).
    async fn send(&mut self, data: (ByteBuffer, tokio::sync::oneshot::Sender<bool>)) -> bool {
        let mut byte_vec = Vec::new();
        let mut acked = 0u64;
        self.last_tid += 1;
        let (vec, tx_result) = data;
        // split bundle data into chunks the size of remote maximum segment size
        for bytes in vec.chunks(self.data_remote.segment_mru as usize) {
            let buf = Bytes::copy_from_slice(bytes);
            let len = buf.len() as u64;
            debug!("bytes len {}", len);
            let packet_data = XferSegData {
                flags: XferSegmentFlags::empty(),
                buf,
                len,
                tid: self.last_tid,
            };
            byte_vec.push(packet_data);
        }
        if byte_vec.is_empty() {
            warn!("Emtpy bundle transfer, aborting");
            if tx_result.send(false).is_err() {
                error!("Internal result channel error");
                return true;
            }
            return false;
        }
        byte_vec.first_mut().unwrap().flags |= XferSegmentFlags::START;
        byte_vec.last_mut().unwrap().flags |= XferSegmentFlags::END;
        // push packets to send task
        for packet in byte_vec {
            if let Err(err) = self
                .tx_session_outgoing
                .send(TcpClPacket::XferSeg(packet))
                .await
            {
                error!("Internal sender error, {}", err);
                if tx_result.send(false).is_err() {
                    error!("Internal result channel error");
                    return true;
                }
                return true;
            }
        }
        // wait for all acks
        while acked < vec.len() as u64 {
            if let Some(received) = self.rx_session_incoming.recv().await {
                match received {
                    TcpClPacket::XferAck(ack_data) => {
                        if ack_data.tid == self.last_tid {
                            acked = ack_data.len;
                        }
                    }
                    TcpClPacket::XferRefuse(refuse_data) => {
                        warn!("Transfer refused, {:?}", refuse_data.reason);
                        if tx_result.send(false).is_err() {
                            error!("Internal result channel error");
                            return true;
                        }
                        return false;
                    }
                    TcpClPacket::MsgReject(msg_reject_data) => {
                        warn!("Message rejected, {:?}", msg_reject_data.reason);
                        if tx_result.send(false).is_err() {
                            error!("Internal result channel error");
                            return true;
                        }
                        return true;
                    }
                    _ => warn!("Unexpected packet while waiting for acks"),
                }
            }
        }
        debug!("All acked");
        // indicate successful transfer
        if tx_result.send(true).is_err() {
            error!("Internal result channel error");
            return true;
        }
        false
    }
}

struct TcpClReceiver {
    rx_tcp: OwnedReadHalf,
    tx_session_incoming: mpsc::Sender<TcpClPacket>,
    timeout: u16,
}

struct TcpClSender {
    tx_tcp: OwnedWriteHalf,
    rx_session_outgoing: mpsc::Receiver<TcpClPacket>,
    timeout: u16,
}

impl TcpClReceiver {
    /// Run receiver task and check keepalive timeout.
    async fn run(mut self) {
        let mut interval = time::interval(Duration::from_secs(self.timeout.into()));
        interval.tick().await;
        loop {
            tokio::select! {
                    parsed_packet = TcpClPacket::deserialize(&mut self.rx_tcp) => {
                        match parsed_packet {
                            Ok(packet) => {
                                debug!("Received and successfully parsed packet");
                                if let TcpClPacket::KeepAlive = packet {
                                    debug!("Received keepalive");
                                } else {
                                    self.send_packet(packet).await;
                                }
                            }
                            Err(err) => {
                                error!("Failed parsing package: {:?}", err);
                                break;
                            }
                        }
                    }
                    // if above future doesn't complete in keepalive time interval, teardown the session
                    _ = interval.tick() => {
                        debug!("Keepalive timeout");
                        self.send_packet(TcpClPacket::SessTerm(SessTermData {
                            flags: SessTermFlags::empty(),
                            reason: SessTermReasonCode::IdleTimeout,
                        })).await;
                        break;
                }
            }
        }
    }
    async fn send_packet(&mut self, packet: TcpClPacket) {
        if let Err(err) = self.tx_session_incoming.send(packet).await {
            error!("Error while sending via internal channel: {}", err);
        }
    }
}

impl TcpClSender {
    /// Run sender task and check keepalive timeout.
    async fn run(mut self) {
        let mut interval = time::interval(Duration::from_secs(self.timeout.into()));
        interval.tick().await;
        loop {
            tokio::select! {
                packet = self.rx_session_outgoing.recv() => {
                    if let Some(packet) = packet {
                       self.send_packet(&packet).await;
                       if let TcpClPacket::SessTerm(_) = packet {
                           //breaks loop, tasks finished, dropped sender, connection closed
                           break;
                       }
                    }
                }
                // periodically wait for keepalive timeout and send packet if above future doesn't complete first
                _ = interval.tick() => {
                    debug!("Keepalive send");
                    self.send_packet(&TcpClPacket::KeepAlive).await;
                }
            }
        }
    }
    async fn send_packet(&mut self, packet: &TcpClPacket) {
        if let Err(err) = packet.serialize(&mut self.tx_tcp).await {
            error!("Error while serializing packet: {}", err);
        }
        if let Err(err) = self.tx_tcp.flush().await {
            error!("Error while flushing tcp sending queue: {}", err);
        }
    }
}

/// Initial tcp connection.
/// Session not yet established.
struct TcpConnection {
    stream: TcpStream,
    addr: SocketAddr,
}

impl TcpConnection {
    /// Session parameter negotiation
    async fn negotiate_session(&mut self) -> anyhow::Result<(SessInitData, SessInitData)> {
        let node_id = (*CONFIG.lock()).host_eid.to_string();
        let mut sess_init_data = SessInitData {
            keepalive: KEEPALIVE,
            segment_mru: SEGMENT_MRU,
            transfer_mru: TRANSFER_MRU,
            node_id,
        };
        let session_init = TcpClPacket::SessInit(sess_init_data.clone());
        session_init.serialize(&mut self.stream).await?;
        self.stream.flush().await?;
        let response = TcpClPacket::deserialize(&mut self.stream).await?;
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
        self.stream.write(b"dtn!").await?;
        self.stream.write_u8(4).await?;
        self.stream.write_u8(flags.bits()).await?;
        self.stream.flush().await?;
        Ok(())
    }

    async fn receive_contact_header(&mut self) -> anyhow::Result<ContactHeaderFlags> {
        let mut buf: [u8; 6] = [0; 6];
        self.stream.read_exact(&mut buf).await?;
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
        sessions: &mut impl DerefMut<
            Target = HashMap<SocketAddr, mpsc::Sender<(ByteBuffer, oneshot::Sender<bool>)>>,
        >,
    ) {
        // Phase 1
        debug!("Exchanging contact header, {}", self.addr);
        if let Err(err) = self.exchange_contact_header().await {
            error!(
                "Failed to exchange contact header with {}: {}",
                self.addr, err
            );
        }
        // Phase 2
        debug!("Negotiating session parameters, {}", self.addr);
        match self.negotiate_session().await {
            Ok((local_parameters, remote_parameters)) => {
                // channel between receiver task and session task, incoming packets
                let (tx_session_incoming, rx_session_incoming) =
                    mpsc::channel::<TcpClPacket>(INTERNAL_CHANNEL_BUFFER);
                // channel between sender task and session task, outgoing packets
                let (tx_session_outgoing, rx_session_outgoing) =
                    mpsc::channel::<TcpClPacket>(INTERNAL_CHANNEL_BUFFER);
                // channel between convergence layer and session, bundle queue
                let (tx_session_queue, rx_session_queue) =
                    mpsc::channel::<(ByteBuffer, oneshot::Sender<bool>)>(INTERNAL_CHANNEL_BUFFER);
                let (rx_tcp, tx_tcp) = self.stream.into_split();
                let rx_task = TcpClReceiver {
                    rx_tcp,
                    tx_session_incoming,
                    timeout: remote_parameters.keepalive,
                };
                let tx_task = TcpClSender {
                    tx_tcp,
                    rx_session_outgoing,
                    timeout: local_parameters.keepalive,
                };
                let session_task = TcpClSession {
                    tx_session_outgoing,
                    rx_session_incoming,
                    rx_session_queue,
                    data_local: local_parameters,
                    data_remote: remote_parameters,
                    last_tid: 0,
                };
                tokio::spawn(rx_task.run());
                tokio::spawn(tx_task.run());
                tokio::spawn(session_task.run());
                // insert new session into session list, provides lookup for bundle sending
                sessions.insert(self.addr, tx_session_queue);
                info!("Started TCP session for {}", self.addr);
            }
            Err(err) => error!("Failed to negotiate session: {}", err),
        }
    }
}

pub struct Listener {
    tcp_listener: TcpListener,
    sessions: Arc<Mutex<SessionMap>>,
}

impl Listener {
    async fn run(self) {
        loop {
            match self.tcp_listener.accept().await {
                Ok((stream, addr)) => {
                    info!("Incoming connection from: {:?}", addr);
                    let connection = TcpConnection { stream, addr };
                    // establish session and insert into shared session list
                    connection.connect(&mut self.sessions.lock().await).await;
                }
                Err(e) => {
                    error!("Couldn't get client: {:?}", e)
                }
            }
        }
    }
}

impl TcpConvergenceLayer {
    pub fn new(port: Option<u16>) -> TcpConvergenceLayer {
        TcpConvergenceLayer {
            local_port: port.unwrap_or(4556),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            listener: Arc::new(Mutex::new(None)),
        }
    }

    /// Send bundles via an existing session
    /// Session should have been established in advance.
    async fn send_bundles(
        &self,
        addr: &SocketAddr,
        ready: &[ByteBuffer],
        sessions: &mut impl DerefMut<Target = SessionMap>,
    ) -> anyhow::Result<bool> {
        if let Some(sender) = sessions.get(addr) {
            debug!("Using existing session for {}", addr);
            let mut results = Vec::new();
            for bundle in ready {
                debug!("Sending bundle {:?}", bundle);
                // unfortunately not possible to avoid cloning, atomic reference counting would be needed in API
                // backchannel that responds whether bundle send was successful
                let (tx, rx) = oneshot::channel::<bool>();
                sender.send((bundle.clone(), tx)).await?;
                let successful = rx.await?;
                results.push(successful);
            }
            for result in results {
                if !result {
                    return Ok(false);
                }
            }
            Ok(true)
        } else {
            warn!("No session found for {} but expected", addr);
            Ok(false)
        }
    }
}

type SessionMap = HashMap<SocketAddr, mpsc::Sender<(ByteBuffer, oneshot::Sender<bool>)>>;

#[derive(Clone, Default, Debug)]
pub struct TcpConvergenceLayer {
    local_port: u16,
    // Session list, new sessions can either be added by [`Listener`] or [`scheduled_submission`]
    sessions: Arc<Mutex<SessionMap>>,
    listener: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[async_trait]
impl ConvergenceLayerAgent for TcpConvergenceLayer {
    async fn setup(&mut self) {
        let tcp_listener = TcpListener::bind(("0.0.0.0", self.local_port))
            .await
            .expect("Couldn't create TCP listener");
        let listener = Listener {
            tcp_listener,
            sessions: self.sessions.clone(),
        };
        *self.listener.lock().await = Some(tokio::spawn(listener.run()));
    }

    fn port(&self) -> u16 {
        self.local_port
    }

    fn name(&self) -> &'static str {
        "tcp"
    }

    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
        let addr = dest.parse().unwrap();
        // lock has to be held across querying and inserting a new session
        let mut session_lock = self.sessions.lock().await;

        if session_lock.get(&addr).is_none() {
            info!("New connection to {}", dest);
            match TcpStream::connect(addr).await {
                Ok(stream) => {
                    let connection = TcpConnection { stream, addr };
                    connection.connect(&mut session_lock).await;
                }
                Err(err) => {
                    warn!("Error connecting to {}, {:?}", dest, err);
                    return false;
                }
            }
        }
        match self.send_bundles(&addr, ready, &mut session_lock).await {
            Ok(result) => result,
            Err(err) => {
                warn!("error while sending bundles: {}", err);
                session_lock.remove(&addr);
                false
            }
        }
    }
}

impl std::fmt::Display for TcpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "tcp")
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
        let mut data_raw: Vec<u8> = Vec::with_capacity(data_len as usize);
        for _ in 0..data_len {
            data_raw.push(0x90);
        }

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

    #[test]
    fn serialize_deserialize() {
        let segs =
            perform_gen_xfer_segs_test(10, 100, 10).expect("error generating xfer segment list");
        for s in segs {
            let mut buf = Vec::new();
            let packet = TcpClPacket::XferSeg(s);
            block_on(packet.serialize(&mut buf)).unwrap();
            let mut slice = buf.as_ref();
            let result = block_on(TcpClPacket::deserialize(&mut slice)).unwrap();
            dbg!(&packet);
            dbg!(&buf);
            dbg!(&result);
            assert!(packet == result);
        }
    }
}
