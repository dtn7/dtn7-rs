use super::ConvergenceLayerAgent;
use actix::clock::delay_for;
use async_trait::async_trait;
use bp7::ByteBuffer;
//use futures_util::stream::StreamExt;
use log::{debug, error, info, warn};
use std::io::Write;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
//use std::net::TcpStream;
use anyhow::{anyhow, bail};
use bytes::BufMut;
use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time::Duration;

use crate::cla::tcpcl::*;

#[derive(Debug, PartialEq)]
enum State {
    Unconnected,
    Setup,
    Idle,
    Sending,
    Receiving,
    Teardown,
    Disconnected,
}
struct Connection {
    stream: TcpStream,
    buffer: bytes::BytesMut,
    state: State,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Connection {
        Connection {
            stream,
            //buffer: bytes::BytesMut::with_capacity(4096*64),
            buffer: bytes::BytesMut::with_capacity(4096),
            state: State::Unconnected,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TcpConvergenceLayer {
    counter: u64,
    local_port: u16,
    remote_config: SessInitData,
}

impl TcpConvergenceLayer {
    pub fn new(port: Option<u16>) -> TcpConvergenceLayer {
        TcpConvergenceLayer {
            counter: 0,
            local_port: port.unwrap_or(4556),
            remote_config: SessInitData {
                keepalive: 30,
                segment_mru: 64000,
                transfer_mru: 64000,
                node_id: "".into(),
            },
        }
    }
    async fn run(mut self) -> Result<(), io::Error> {
        let addr: SocketAddrV4 = format!("0.0.0.0:{}", self.port()).parse().unwrap();
        let mut listener = TcpListener::bind(&addr).await?;
        //tokio::spawn({ client_connect("127.0.0.1:4223".parse().unwrap()) });
        debug!("spawning TCP listener on port {}", self.port(),);
        loop {
            let (mut stream, remote) = listener
                .accept()
                .await
                .expect("error accepting TCPCL connection");

            let peer_addr = stream.peer_addr().unwrap();
            info!("Incoming connection from {}", peer_addr);

            // Phase 1: Exchange Contact Header
            debug!("CH <-");
            let res = receive_contact_header(&mut stream).await;
            if res.is_err() {
                debug!("received error: {:?}", res);
                send_sess_term(
                    &mut stream,
                    SessTermReasonCode::VersionMismatch,
                    SessTermFlags::empty(),
                )
                .await;
                continue;
            }
            let ch_flags = res.unwrap();
            debug!("CH ->");
            if send_contact_header(&mut stream).await.is_err() {
                continue;
            };

            debug!("exchanged contact headers");
            // Phase 2: Negotiate

            let mut buffer: bytes::BytesMut = bytes::BytesMut::new();
            stream.read_buf(&mut buffer).await?;
            debug!("The bytes: {:?}", &buffer[..]);
            let res = parses_packet(&mut buffer).await;
            debug!("parsed: {:?}", res);
            debug!("The bytes: {:?}", &buffer[..]);
            if let Ok(TcpClPacket::SessInit(data)) = res {
                self.remote_config = data;
            }
            let sess_init_data = SessInitData {
                keepalive: 30,
                segment_mru: 64000,
                transfer_mru: 64000,
                node_id: "test2".into(),
            };
            send_sess_init(&mut stream, sess_init_data).await;

            // Phase 3: Idle

            debug!(
                "(SERVER) communication ended: {:?}",
                self.communicate(&mut buffer, stream).await
            );

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

    pub async fn spawn_listener(&self) -> std::io::Result<()> {
        let self2 = self.clone();
        tokio::spawn(self2.run());
        Ok(())
    }
    pub async fn connect(&mut self, addr: SocketAddr) -> anyhow::Result<()> {
        debug!("client connecting via stream");
        if let Ok(mut stream) = TcpStream::connect(&addr).await {
            debug!("sending CH");
            send_contact_header(&mut stream).await?;

            debug!("receiving CH");
            let res = receive_contact_header(&mut stream).await;
            if res.is_err() {
                debug!("received error: {:?}", res);
                send_sess_term(
                    &mut stream,
                    SessTermReasonCode::VersionMismatch,
                    SessTermFlags::empty(),
                )
                .await?;
            } else {
                let ch_flags = res.unwrap();

                debug!("got flags: {:?}", ch_flags);

                debug!("sending SESS_INIT");
                let sess_init_data = SessInitData {
                    keepalive: 30,
                    segment_mru: 64000,
                    transfer_mru: 64000,
                    node_id: "test1".into(),
                };
                send_sess_init(&mut stream, sess_init_data).await?;

                let mut buffer: bytes::BytesMut = bytes::BytesMut::new();
                stream.read_buf(&mut buffer).await?;
                debug!("The bytes: {:?}", &buffer[..]);
                let res = parses_packet(&mut buffer).await;
                debug!("parsed: {:?}", res);
                debug!("The bytes: {:?}", &buffer[..]);
                if let Ok(TcpClPacket::SessInit(data)) = res {
                    debug!("yay");
                    self.remote_config = data;
                    debug!(
                        "(CLIENT) communication ended: {:?}",
                        self.communicate(&mut buffer, stream).await
                    );
                }
            }
        } else {
            debug!("error connceting to peer: {:?}", addr);
        }
        Ok(())
    }
    /*async fn msg_writer(
        mut w: tokio::io::WriteHalf<&mut TcpStream>,
        mut rx: Receiver<TcpClPacket>,
    ) {
        while let Some(pkt) = rx.recv().await {
            println!("got = {:?}", pkt);
            w.write_all(b"test");
        }
    }*/
    async fn communicate(
        &mut self,
        mut buffer: &mut bytes::BytesMut,
        mut stream: TcpStream,
    ) -> anyhow::Result<()> {
        let mut state = State::Idle;

        let mut connected = true;
        //let (mut reader, mut writer) = tokio::io::split(stream);
        let (mut reader, mut writer) = stream.into_split();
        let (mut tx, mut rx) = channel::<TcpClPacket>(50);
        tokio::spawn(async move {
            //self::msg_writer(mut writer, mut rx).await;
            while let Some(pkt) = rx.recv().await {
                debug!("got = {:?}", pkt);
                //writer.write_all(b"test");
                match pkt {
                    TcpClPacket::KeepAlive => {
                        send_keepalive(&mut writer).await;
                    }
                    _ => {}
                }
            }
        });
        {
            let ka_interval = self.remote_config.keepalive;
            let mut tx = tx.clone();
            tokio::spawn(async move {
                //self::msg_writer(mut writer, mut rx).await;
                loop {
                    debug!("keepalive delay: {}s", ka_interval);
                    delay_for(Duration::from_secs(ka_interval as u64)).await;
                    debug!("keepalive send");
                    if tx.send(TcpClPacket::KeepAlive).await.is_err() {
                        break;
                    }
                }
            });
        }
        let mut seg_data = bytes::BytesMut::new();
        let mut active_tid: Option<u64> = None;
        let mut ack_sizes: Vec<u64> = Vec::new();
        let mut all_sent = false;

        while connected {
            debug!("looping");
            reader.read_buf(&mut buffer).await?;
            let res = parses_packet(&mut buffer).await;
            if let Err(err) = res {
                match err {
                    TcpClError::NotEnoughBytesReceived => continue,
                    _ => {
                        debug!("tcp cl packet parsing error: {:?}", err);
                        connected = false;
                    }
                }
            } else if let Ok(pkt) = res {
                match pkt {
                    TcpClPacket::KeepAlive => {
                        //unimplemented!();
                        debug!("got keepalive");
                    }
                    TcpClPacket::SessTerm(data) => {
                        unimplemented!();
                    }
                    TcpClPacket::XferSeg(data) => {
                        if state != State::Idle || state != State::Receiving {
                            warn!("unexpected xfer seg received, terminating session");
                            unimplemented!();
                        }
                        if state == State::Idle {
                            state = State::Receiving;
                            active_tid = Some(data.tid);
                        }
                        if state == State::Receiving {
                            if active_tid != Some(data.tid) {
                                warn!("unexpectid transfer id received, terminating session");
                                unimplemented!();
                            }
                        }
                        seg_data.put(data.buf);
                        let ack = XferAckData {
                            flags: data.flags,
                            tid: data.tid,
                            len: seg_data.len() as u64,
                        };
                        if data.flags.contains(XferSegmentFlags::END) {
                            debug!("received compelete bundle!");
                            state = State::Idle;
                            active_tid = None;
                            // TODO: handle complete bundle
                        }
                        if tx.send(TcpClPacket::XferAck(ack)).await.is_err() {
                            connected = false;
                        }
                    }
                    TcpClPacket::XferAck(data) => {
                        if state != State::Sending || ack_sizes.is_empty() || active_tid.is_none() {
                            warn!("unexpected xfer ack received, terminating session");
                            unimplemented!();
                        }
                        if active_tid != Some(data.tid) {
                            warn!("unexpected xfer ack tranfer ID received, terminating session");
                            unimplemented!();
                        }
                        if ack_sizes[0] != data.len {
                            warn!("unexpected xfer ack length, terminating session");
                            unimplemented!();
                        }
                        if ack_sizes.is_empty() && all_sent {
                            debug!("sent bundle was received completley");
                            all_sent = false;
                            state = State::Idle;
                            active_tid = None;
                        }
                    }
                    TcpClPacket::XferRefuse(data) => {
                        unimplemented!();
                    }
                    TcpClPacket::MsgReject => {
                        unimplemented!();
                    }
                    _ => {
                        debug!("unexpected packet received: {:?}", pkt);
                        connected = false;
                    }
                }
            }
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
            let mut self2 = self.clone();
            self2.connect(peeraddr).await;
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
