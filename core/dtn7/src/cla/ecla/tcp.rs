use super::Connector;
use crate::cla::ecla::processing::{handle_connect, handle_disconnect, handle_packet};
use crate::cla::ecla::Packet;
use crate::lazy_static;
use async_trait::async_trait;
use futures_util::{future, stream::TryStreamExt};
use log::info;
use log::{debug, error};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};

type TCPConnection = super::Connection<Vec<u8>>;
type PeerMap = Arc<Mutex<HashMap<String, TCPConnection>>>;

lazy_static! {
    static ref PEER_MAP: PeerMap = PeerMap::new(Mutex::new(HashMap::new()));
}

// Handles the TCP connection.
async fn handle_connection(raw_stream: TcpStream, addr: SocketAddr) {
    info!("Incoming TCP connection from: {}", addr);

    let (tx, mut rx) = mpsc::channel(100);
    let (tx_close, rx_close) = oneshot::channel();

    // Insert the write part of this peer to the peer map.
    PEER_MAP.lock().unwrap().insert(
        addr.to_string(),
        TCPConnection {
            tx,
            close: Some(tx_close),
        },
    );
    handle_connect("TCP".to_string(), addr.to_string());

    let (incoming, outgoing) = raw_stream.into_split();

    // Delimit frames using a length header
    let length_delimited = FramedRead::new(incoming, LengthDelimitedCodec::new());

    // Deserialize frames
    let deserialized = tokio_serde::SymmetricallyFramed::new(
        length_delimited,
        SymmetricalJson::<Packet>::default(),
    );

    let incoming = deserialized.try_for_each(|packet| {
        handle_packet("TCP".to_string(), addr.to_string(), packet);
        future::ok(())
    });

    let outgoing = tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            if let Err(err) = outgoing.try_write(cmd.as_slice()) {
                error!("err while sending to outgoing channel: {}", err);
            }
        }
    });

    // Wait for the incoming and outgoing channel to close or
    // until a close command for this connection is received.
    future::select(rx_close, future::select(incoming, outgoing)).await;

    if PEER_MAP.lock().unwrap().remove(&addr.to_string()).is_some() {
        info!("ECLA (TCP) {} disconnected", &addr);
        handle_disconnect(addr.to_string());
    }
}

#[derive(Clone, Default)]
pub struct TCPConnector {
    port: u16,
}

impl TCPConnector {
    pub fn new(port: u16) -> TCPConnector {
        TCPConnector { port }
    }
}

#[async_trait]
impl Connector for TCPConnector {
    async fn setup(&mut self) {
        let port = self.port;

        tokio::spawn(async move {
            let addr = String::from("127.0.0.1:") + port.to_string().as_str();

            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!("External Convergence Layer TCP Listening on: {}", addr);

            // Let's spawn the handling of each connection in a separate task.
            while let Ok((stream, addr)) = listener.accept().await {
                tokio::spawn(handle_connection(stream, addr));
            }
        });
    }

    fn name(&self) -> &str {
        "TCP"
    }

    fn send_packet(&self, dest: &str, packet: &Packet) -> bool {
        debug!("Sending Packet to {} ({})", dest, self.name());

        let peer_map = PEER_MAP.lock().unwrap();
        let target = peer_map.get(dest);
        if target.is_some() {
            // Build the packet frame [ len: u32 | frame payload (data) ]
            let mut data = serde_json::to_vec(&packet).unwrap();
            let len = (data.len() as u32).to_be_bytes();
            data.splice(0..0, len.iter().cloned());

            if let Some(target) = target {
                return target.tx.try_send(data).is_ok();
            }
        }

        false
    }

    fn close(&self, addr: &str) {
        if let Some(conn) = PEER_MAP.lock().unwrap().get_mut(addr) {
            let close = conn.close.take();
            if let Err(_err) = close.unwrap().send(()) {
                debug!("Error while sending close to {}", addr);
            }
        }
    }
}
