use super::TransportLayer;
use crate::dtnd::ecla::processing::{handle_connect, handle_disconnect, handle_packet};
use crate::dtnd::ecla::Packet;
use crate::lazy_static;
use async_trait::async_trait;
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use log::debug;
use log::info;
use serde_json::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

struct Connection {
    tx: Tx,
}

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<String, Connection>>>;

lazy_static! {
    static ref PEER_MAP: PeerMap = PeerMap::new(Mutex::new(HashMap::new()));
}

// Handles the websocket connection.
async fn handle_connection(raw_stream: TcpStream, addr: SocketAddr) {
    info!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");

    info!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = unbounded();
    PEER_MAP
        .lock()
        .unwrap()
        .insert(addr.to_string(), Connection { tx });
    handle_connect("Websocket".to_string(), addr.to_string());

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        info!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap().trim()
        );

        let packet: Result<Packet>;
        {
            // Get own peer
            let mut pmap = PEER_MAP.lock().unwrap();

            let me_opt = pmap.get_mut(&addr.to_string());
            if me_opt.is_none() {
                return future::ok(());
            }

            // Deserialize Packet
            packet = serde_json::from_str(msg.to_text().unwrap());
            if packet.is_err() {
                return future::ok(());
            }
        }

        handle_packet("Websocket".to_string(), addr.to_string(), packet.unwrap());

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    info!("{} disconnected", &addr);
    handle_disconnect(addr.to_string());
    PEER_MAP.lock().unwrap().remove(&addr.to_string());
}

#[derive(Clone, Default)]
pub struct WebsocketTransportLayer {
    port: u16,
}

impl WebsocketTransportLayer {
    pub fn new(port: u16) -> WebsocketTransportLayer {
        WebsocketTransportLayer { port }
    }
}

#[async_trait]
impl TransportLayer for WebsocketTransportLayer {
    async fn setup(&mut self) {
        debug!("Setup Websocket ECLA Layer");

        let port = self.port;
        tokio::spawn(async move {
            let addr = String::from("127.0.0.1:") + port.to_string().as_str();

            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!(
                "External Convergence Layer Websocket Listening on: {}",
                addr
            );

            // Let's spawn the handling of each connection in a separate task.
            while let Ok((stream, addr)) = listener.accept().await {
                tokio::spawn(handle_connection(stream, addr));
            }
        });
    }

    fn name(&self) -> &str {
        return "Websocket";
    }

    fn send_packet(&self, dest: &str, packet: &Packet) -> bool {
        debug!("Sending Packet to {} ({})", dest, self.name());

        let pmap = PEER_MAP.lock().unwrap();
        let target = pmap.get(dest);
        if target.is_some() {
            let data = serde_json::to_string(&packet);
            return target
                .unwrap()
                .tx
                .unbounded_send(Message::Text(data.unwrap()))
                .is_ok();
        }

        return false;
    }

    fn close(&self, dest: &str) {
        todo!()
    }
}
