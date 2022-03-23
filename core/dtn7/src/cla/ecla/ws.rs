use super::TransportLayer;
use crate::cla::ecla::processing::{handle_connect, handle_disconnect, handle_packet};
use crate::cla::ecla::Packet;
use crate::lazy_static;
use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use log::info;
use log::{debug, trace};
use serde_json::Result;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Connection represents the session of a connection with a Tx channel to send data
/// and a oneshot channel to signal a closing of the session once.
struct Connection {
    tx: Tx,
    close: Option<oneshot::Sender<()>>,
}

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<String, Connection>>>;

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

    let (tx, rx) = unbounded();
    let (tx_close, rx_close) = oneshot::channel();

    PEER_MAP.lock().unwrap().insert(
        id.to_string(),
        Connection {
            tx,
            close: Some(tx_close),
        },
    );
    handle_connect(LAYER_NAME.to_string(), id.to_string());

    let (outgoing, incoming) = ws.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        trace!(
            "Received a message from {}: {}",
            id,
            msg.to_text().unwrap().trim()
        );

        let packet: Result<Packet>;
        {
            // Get own peer
            let mut peer_map = PEER_MAP.lock().unwrap();

            let me_opt = peer_map.get_mut(&id.to_string());
            if me_opt.is_none() {
                return future::ok(());
            }

            // Deserialize Packet
            packet = serde_json::from_str(msg.to_text().unwrap());
            if packet.is_err() {
                return future::ok(());
            }
        }

        handle_packet(LAYER_NAME.to_string(), id.to_string(), packet.unwrap());

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others, rx_close);
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
pub struct WebsocketTransportLayer {}

impl WebsocketTransportLayer {
    pub fn new() -> WebsocketTransportLayer {
        WebsocketTransportLayer {}
    }
}

#[async_trait]
impl TransportLayer for WebsocketTransportLayer {
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
            return target
                .tx
                .unbounded_send(Message::Text(data.unwrap()))
                .is_ok();
        }

        false
    }

    fn close(&self, addr: &str) {
        if let Some(conn) = PEER_MAP.lock().unwrap().get_mut(addr) {
            let close = std::mem::replace(&mut conn.close, None);
            if let Err(_err) = close.unwrap().send(()) {
                debug!("Error while sending close to {}", addr);
            }
        }
    }
}