use super::TransportLayer;
use crate::dtnd::ecla::processing::{handle_connect, handle_disconnect, handle_packet};
use crate::dtnd::ecla::Packet;
use crate::lazy_static;
use async_trait::async_trait;
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use log::info;
use log::{debug, error};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};

struct Connection {
    tx: Tx,
}

type Tx = UnboundedSender<Vec<u8>>;
type PeerMap = Arc<Mutex<HashMap<String, Connection>>>;

lazy_static! {
    static ref PEER_MAP: PeerMap = PeerMap::new(Mutex::new(HashMap::new()));
}

// Handles the websocket connection.
async fn handle_connection(mut raw_stream: TcpStream, addr: SocketAddr) {
    info!("Incoming TCP connection from: {}", addr);

    let (tx, rx) = unbounded();

    // Insert the write part of this peer to the peer map.
    PEER_MAP
        .lock()
        .unwrap()
        .insert(addr.to_string(), Connection { tx });
    handle_connect("TCP".to_string(), addr.to_string());

    let (incoming, outgoing) = raw_stream.split();

    // Delimit frames using a length header
    let length_delimited = FramedRead::new(incoming, LengthDelimitedCodec::new());

    // Deserialize frames
    let deserialized = tokio_serde::SymmetricallyFramed::new(
        length_delimited,
        SymmetricalJson::<Packet>::default(),
    );

    let broadcast_incoming = deserialized.try_for_each(|packet| {
        handle_packet("TCP".to_string(), addr.to_string(), packet);
        future::ok(())
    });

    let broadcast_outgoing = rx.for_each(|packet| {
        if let Err(err) = outgoing.try_write(packet.as_slice()) {
            error!("error while sending packet ({})", err);
        }
        future::ready(())
    });

    pin_mut!(broadcast_incoming, broadcast_outgoing);
    future::select(broadcast_incoming, broadcast_outgoing).await;

    if PEER_MAP.lock().unwrap().remove(&addr.to_string()).is_some() {
        info!("{} disconnected", &addr);
        handle_disconnect(addr.to_string());
    }
}

#[derive(Clone, Default)]
pub struct TCPTransportLayer {
    port: u16,
}

impl TCPTransportLayer {
    pub fn new(port: u16) -> TCPTransportLayer {
        TCPTransportLayer { port }
    }
}

#[async_trait]
impl TransportLayer for TCPTransportLayer {
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

        let pmap = PEER_MAP.lock().unwrap();
        let target = pmap.get(dest);
        if target.is_some() {
            let mut data = serde_json::to_vec(&packet).unwrap();
            let len = (data.len() as i32).to_be_bytes();
            data.splice(0..0, len.iter().cloned());

            if let Some(target) = target {
                if let Ok(()) = target.tx.unbounded_send(data) {
                    return true;
                }
            }
        }

        false
    }

    fn close(&self, _dest: &str) {
        // Todo: closing of client
    }
}
