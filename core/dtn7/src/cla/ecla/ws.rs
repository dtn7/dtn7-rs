use super::Connector;
use crate::cla::ecla::processing::{handle_connect, handle_disconnect, handle_packet};
use crate::cla::ecla::Packet;
use crate::lazy_static;
use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{future, stream::TryStreamExt, SinkExt, StreamExt};
use log::{debug, error, info, trace, warn};
use serde_json::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::sync::oneshot;

type WebSocketConnection = super::Connection<Message>;
type PeerMap = Arc<Mutex<HashMap<String, WebSocketConnection>>>;

lazy_static! {
    /// Tracks the connected peers (modules)
    static ref PEER_MAP: PeerMap = PeerMap::new(Mutex::new(HashMap::new()));
}

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
static LAYER_NAME: &str = "Websocket";

/// Handles the websocket connection coming from httpd
pub async fn handle_connection(ws: WebSocket) {
    // We can't get a remote address from ws so we create own monotonic increasing id's
    let id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    let (tx, mut rx) = mpsc::channel(100);
    let (tx_close, rx_close) = oneshot::channel();

    PEER_MAP.lock().unwrap().insert(
        id.to_string(),
        WebSocketConnection {
            tx,
            close: Some(tx_close),
        },
    );
    handle_connect(LAYER_NAME.to_string(), id.to_string());

    let (mut outgoing, incoming) = ws.split();

    // Process incoming messages from the websocket client
    let broadcast_incoming = incoming.try_for_each(|msg| {
        let packet: Result<Packet>;
        {
            // Get own peer
            let mut peer_map = PEER_MAP.lock().unwrap();

            let me_opt = peer_map.get_mut(&id.to_string());
            if me_opt.is_none() {
                return future::ok(());
            }

            // Try to convert the message to text
            let msg_text = match msg.to_text() {
                Ok(text) => {
                    trace!("Received a message from ECLA id {}: {}", id, text.trim());
                    text.trim()
                }
                Err(e) => {
                    warn!(
                        "Failed to convert message to text from ECLA id {}: {}",
                        id, e
                    );
                    return future::ok(());
                }
            };

            // Deserialize Packet
            packet = serde_json::from_str(msg_text);
            if packet.is_err() {
                return future::ok(());
            }
        }

        handle_packet(LAYER_NAME.to_string(), id.to_string(), packet.unwrap());

        future::ok(())
    });

    // Pass the received messages to the websocket client.
    let receive_from_others = tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            if let Err(err) = outgoing.send(cmd).await {
                error!("err while sending to outgoing channel: {}", err);
            }
        }
    });

    // Wait for the broadcast incoming and outgoing channel to close or
    // until a close command for this connection is received.
    future::select(
        broadcast_incoming,
        future::select(receive_from_others, rx_close),
    )
    .await;

    info!("{} disconnected", id);
    handle_disconnect(id.to_string());
    PEER_MAP.lock().unwrap().remove(&id.to_string());
}

#[derive(Clone, Default)]
pub struct WebsocketConnector {}

impl WebsocketConnector {
    pub fn new() -> WebsocketConnector {
        WebsocketConnector {}
    }
}

#[async_trait]
impl Connector for WebsocketConnector {
    async fn setup(&mut self) {
        // Because we use the server in httpd we don't have any setup
    }

    fn name(&self) -> &str {
        "Websocket"
    }

    fn send_packet(&self, dest: &str, packet: &Packet) -> bool {
        debug!("Sending Packet to {} ({})", dest, self.name());

        let peer_map = PEER_MAP.lock().unwrap();
        if let Some(target) = peer_map.get(dest) {
            let data = serde_json::to_string(&packet);
            return target.tx.try_send(Message::Text(data.unwrap())).is_ok();
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
