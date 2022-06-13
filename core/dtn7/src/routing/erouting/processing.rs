use super::{
    DroppedPeer, EncounteredPeer, IncomingBundle, IncomingBundleWithoutPreviousNode, Packet,
    PeerState, RequestSenderForBundle, SenderForBundleResponse, SendingFailed, ServiceState,
};
use crate::cla::ConvergenceLayerAgent;
use crate::{
    cla_names, lazy_static, peers_get_for_node, service_add, BundlePack, ClaSenderTask,
    RoutingNotifcation, CLAS, DTNCORE, PEERS,
};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{future, SinkExt, StreamExt, TryStreamExt};
use log::{error, info, trace};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
use tokio::time::timeout;

/// Maximum timeout for a sender_for_bundle response packet.
const EROUTING_RESPONSE_TIMEOUT_MS: u64 = 250;

/// Holds the channel to send messages to the connected router.
struct Connection {
    tx: Sender<Message>,
}

type ResponseMap = Arc<Mutex<HashMap<String, oneshot::Sender<Packet>>>>;

lazy_static! {
    /// Keeps track of the single router that can be connected.
    static ref CONNECTION: Arc<Mutex<Option<Connection>>> = Arc::new(Mutex::new(None));
    /// Tracks the response channels for SenderForBundle requests.
    static ref RESPONSES: ResponseMap = ResponseMap::new(Mutex::new(HashMap::new()));
}

fn send_peer_state() {
    let peer_state: Packet = Packet::PeerState(PeerState {
        peers: PEERS.lock().clone(),
    });
    send_packet(&peer_state);
}

fn send_service_state() {
    let service_state: Packet = Packet::ServiceState(ServiceState {
        service_list: DTNCORE.lock().service_list.clone(),
    });
    send_packet(&service_state);
}

pub async fn handle_connection(ws: WebSocket) {
    let (tx, mut rx) = mpsc::channel(100);

    if CONNECTION.lock().unwrap().is_some() {
        info!("Websocket connection closed because external routing agent is already connected");
        if let Err(err) = ws.close().await {
            info!("Error while closing websocket: {}", err);
        }
        return;
    }

    *CONNECTION.lock().unwrap() = Some(Connection { tx });

    // Send initial states to the router
    send_peer_state();
    send_service_state();

    let (mut outgoing, incoming) = ws.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        trace!(
            "Received a external routing message: {}",
            msg.to_text().unwrap().trim()
        );

        let packet: serde_json::Result<Packet> = serde_json::from_str(msg.to_text().unwrap());

        match packet {
            Ok(packet) => match packet {
                // When a SenderForBundleResponse is received we check if a response channel for that
                // bundle id exists and send the response on that channel.
                Packet::SenderForBundleResponse(packet) => {
                    trace!(
                        "sender_for_bundle response: {}",
                        msg.to_text().unwrap().trim()
                    );

                    if let Some(tx) = RESPONSES
                        .lock()
                        .unwrap()
                        .remove(packet.bp.to_string().as_str())
                    {
                        if tx.send(Packet::SenderForBundleResponse(packet)).is_err() {
                            error!("sender_for_bundle response could not be passed to channel")
                        }
                    } else {
                        info!("sender_for_bundle no response channel available")
                    }
                }
                // Add a service on packet
                Packet::ServiceAdd(packet) => {
                    info!(
                        "adding service via erouting {}:{}",
                        packet.tag, packet.service
                    );

                    service_add(packet.tag, packet.service);
                }
                _ => {}
            },
            Err(err) => {
                info!("err decoding external routing packet: {}", err);
            }
        }

        future::ok(())
    });

    let receive_from_others = tokio::spawn(async move {
        while let Some(cmd) = rx.recv().await {
            if let Err(err) = outgoing.send(cmd).await {
                error!("err while sending to outgoing channel: {}", err);
            }
        }
    });

    future::select(broadcast_incoming, receive_from_others).await;

    info!("External routing disconnected");
    disconnect();
}

fn disconnect() {
    (*CONNECTION.lock().unwrap()) = None;
}

/// Sends a JSON encoded packet to the connected router.
fn send_packet(p: &Packet) {
    if let Ok(data) = serde_json::to_string(p) {
        if let Some(con) = CONNECTION.lock().unwrap().as_ref() {
            con.tx
                .try_send(Message::Text(data))
                .expect("error while sending to tx");
        }
    }
}

/// Takes the RoutingNotification's, encodes them to serializable structs and then sends them
/// to the external router if one is connected.
pub fn notify(notification: RoutingNotifcation) {
    let packet: Packet = match notification {
        RoutingNotifcation::SendingFailed(bid, cla_sender) => {
            Packet::SendingFailed(SendingFailed { bid, cla_sender })
        }
        RoutingNotifcation::IncomingBundle(bndl) => Packet::IncomingBundle(IncomingBundle { bndl }),
        RoutingNotifcation::IncomingBundleWithoutPreviousNode(bid, node_name) => {
            Packet::IncomingBundleWithoutPreviousNode(IncomingBundleWithoutPreviousNode {
                bid,
                node_name,
            })
        }
        RoutingNotifcation::EncounteredPeer(eid) => Packet::EncounteredPeer(EncounteredPeer {
            name: eid.node().unwrap(),
            eid: eid.clone(),
            peer: peers_get_for_node(&eid).unwrap(),
        }),
        RoutingNotifcation::DroppedPeer(eid) => Packet::DroppedPeer(DroppedPeer {
            name: eid.node().unwrap(),
            eid,
        }),
    };

    send_packet(&packet);
}

fn remove_response_channel(id: &str) {
    RESPONSES.lock().unwrap().remove(id);
}

fn create_response_channel(id: &str, tx: oneshot::Sender<Packet>) {
    RESPONSES.lock().unwrap().insert(id.to_string(), tx);
}

// Builds a list of ClaSenderTask from the information contained in the SenderForBundleResponse packet.
fn unpack_sender_for_bundle(packet: SenderForBundleResponse) -> (Vec<ClaSenderTask>, bool) {
    (
        packet
            .clas
            .iter()
            .filter_map(|sender| {
                for cla_instance in &(*CLAS.lock()) {
                    // Search for the CLA from the packet by name.
                    if sender.agent == cla_instance.name() {
                        let dest = format!(
                            "{}:{}",
                            sender.remote,
                            sender.port.unwrap_or_else(|| cla_instance.port())
                        );

                        return Some(ClaSenderTask {
                            tx: cla_instance.channel(),
                            dest,
                            cla_name: cla_instance.name().into(),
                            next_hop: sender.next_hop.clone(),
                        });
                    }
                }
                None
            })
            .collect(),
        packet.delete_afterwards,
    )
}

/// Tries to send a routing requests to the external router and waits for the response.
/// The wait will be limited to a timeout of 250ms.
pub async fn sender_for_bundle(bp: &BundlePack) -> (Vec<ClaSenderTask>, bool) {
    trace!("external sender_for_bundle initiated: {}", bp);

    if CONNECTION.lock().unwrap().is_none() {
        return (vec![], false);
    }

    // Register a response channel for the request
    let (tx, rx) = oneshot::channel();
    create_response_channel(bp.to_string().as_str(), tx);

    // Send out the SenderForBundle packet
    let packet: Packet = Packet::RequestSenderForBundle(RequestSenderForBundle {
        clas: cla_names(),
        bp: bp.clone(),
    });
    send_packet(&packet);

    let res = timeout(
        time::Duration::from_millis(EROUTING_RESPONSE_TIMEOUT_MS),
        rx,
    )
    .await;
    if let Ok(Ok(Packet::SenderForBundleResponse(packet))) = res {
        remove_response_channel(bp.to_string().as_str());

        if packet.bp.to_string() != bp.to_string() {
            error!("got a wrong bundle pack! {} != {}", bp, packet.bp);
            return (vec![], false);
        }

        return unpack_sender_for_bundle(packet);
    }

    // Signal to the external router that the timeout was reached and no SenderForBundleResponse was processed.
    // This is needed in case that the response arrived later than the timeout and the connected router thinks
    // it successfully send its response. Otherwise there is no way for the router to know if its response has
    // failed.
    send_packet(&Packet::Timeout(super::Timeout { bp: bp.clone() }));

    info!("timeout while waiting for sender_for_bundle");
    remove_response_channel(bp.to_string().as_str());
    (vec![], false)
}
