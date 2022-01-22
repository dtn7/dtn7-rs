use super::{
    DroppedPeerPacket, EncounteredPeerPacket, IncomingBundlePacket,
    IncomingBundleWithoutPreviousNodePacket, Packet, PeerStatePacket, SendForBundlePacket,
    SendingFailedPacket,
};
use crate::cla::ClaSender;
use crate::RoutingNotifcation::{
    DroppedPeer, EncounteredPeer, IncomingBundle, IncomingBundleWithoutPreviousNode, SendingFailed,
};
use crate::{
    cla_names, lazy_static, peers_get_for_node, BundlePack, RoutingNotifcation, CONFIG, PEERS,
};
use axum::extract::ws::{Message, WebSocket};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use log::{debug, info};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::{thread, time};

struct Connection {
    tx: UnboundedSender<Message>,
}

type ResponseMap = Arc<Mutex<HashMap<String, UnboundedSender<Packet>>>>;

lazy_static! {
    static ref CONNECTION: Arc<Mutex<Option<Connection>>> = Arc::new(Mutex::new(None));
    static ref RESPONSES: ResponseMap = ResponseMap::new(Mutex::new(HashMap::new()));
}

pub async fn handle_connection(ws: WebSocket) {
    if CONFIG.lock().routing != "external" {
        info!("Websocket connection closed because external routing is not enabled");
        if let Err(err) = ws.close().await {
            info!("Error while closing websocket: {}", err);
        }
        return;
    }

    let (tx, rx) = unbounded();

    if CONNECTION.lock().unwrap().is_some() {
        info!("Websocket connection closed because external routing agent is already connected");
        if let Err(err) = ws.close().await {
            info!("Error while closing websocket: {}", err);
        }
        return;
    }

    *CONNECTION.lock().unwrap() = Some(Connection { tx });

    let peer_state: Packet = Packet::PeerStatePacket(PeerStatePacket {
        peers: PEERS.lock().clone(),
    });
    send_packet(&peer_state);

    let (outgoing, incoming) = ws.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        info!(
            "Received a external routing message: {}",
            msg.to_text().unwrap().trim()
        );

        let packet: serde_json::Result<Packet> = serde_json::from_str(msg.to_text().unwrap());

        match packet {
            #[allow(clippy::single_match)]
            Ok(packet) => match packet {
                Packet::SendForBundleResponsePacket(packet) => {
                    debug!(
                        "sender_for_bundle response: {}",
                        msg.to_text().unwrap().trim()
                    );

                    if let Some(tx) = RESPONSES
                        .lock()
                        .unwrap()
                        .get(packet.bp.to_string().as_str())
                    {
                        tx.unbounded_send(Packet::SendForBundleResponsePacket(packet))
                            .expect("could not send response to channel");
                    } else {
                        info!("sender_for_bundle response could not be passed")
                    }
                }
                _ => {}
            },
            Err(err) => {
                info!("err decoding external routing packet: {}", err);
            }
        }

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    info!("External routing disconnected");
    disconnect();
}

fn disconnect() {
    if CONNECTION.lock().unwrap().is_some() {
        (*CONNECTION.lock().unwrap()) = None;
    }
}

fn send_packet(p: &Packet) {
    if let Ok(data) = serde_json::to_string(p) {
        if let Some(con) = CONNECTION.lock().unwrap().as_ref() {
            con.tx
                .unbounded_send(Message::Text(data))
                .expect("error while sending to tx");
        }
    }
}

pub fn notify(notification: RoutingNotifcation) {
    let packet: Packet;
    match notification {
        SendingFailed(bid, cla_sender) => {
            packet = Packet::SendingFailedPacket(SendingFailedPacket {
                bid: bid.to_string(),
                cla_sender: cla_sender.to_string(),
            });
        }
        IncomingBundle(bndl) => {
            packet = Packet::IncomingBundlePacket(IncomingBundlePacket { bndl: bndl.clone() });
        }
        IncomingBundleWithoutPreviousNode(bid, node_name) => {
            packet = Packet::IncomingBundleWithoutPreviousNodePacket(
                IncomingBundleWithoutPreviousNodePacket {
                    bid: bid.to_string(),
                    node_name: node_name.to_string(),
                },
            );
        }
        EncounteredPeer(eid) => {
            packet = Packet::EncounteredPeerPacket(EncounteredPeerPacket {
                eid: eid.clone(),
                peer: peers_get_for_node(eid).unwrap(),
            });
        }
        DroppedPeer(eid) => {
            packet = Packet::DroppedPeerPacket(DroppedPeerPacket { eid: eid.clone() });
        }
    }

    send_packet(&packet);
}

fn remove_response_channel(id: &str) {
    RESPONSES.lock().unwrap().remove(id);
}

fn create_response_channel(id: &str, tx: UnboundedSender<Packet>) {
    RESPONSES.lock().unwrap().insert(id.to_string(), tx);
}

pub fn sender_for_bundle(bp: &BundlePack) -> (Vec<ClaSender>, bool) {
    debug!("external sender_for_bundle initiated: {}", bp);

    let (tx, mut rx) = unbounded();
    create_response_channel(bp.to_string().as_str(), tx);

    let packet: Packet = Packet::SendForBundlePacket(SendForBundlePacket {
        clas: cla_names(),
        bp: bp.clone(),
    });
    send_packet(&packet);

    for _ in 0..25 {
        if CONNECTION.lock().unwrap().is_some() {
            if let Ok(Some(Packet::SendForBundleResponsePacket(packet))) = rx.try_next() {
                if packet.bp.to_string() != bp.to_string() {
                    info!("got a wrong bundle pack! {} != {}", bp, packet.bp);
                    continue;
                }

                remove_response_channel(bp.to_string().as_str());
                return (packet.clas, false);
            }
        } else {
            info!("no external routing! no sender_for_bundle possible");

            remove_response_channel(bp.to_string().as_str());
            return (vec![], false);
        }

        thread::sleep(time::Duration::from_millis(100)); // TODO: Make timeout configurable or find better solution
    }

    info!("timeout while waiting for sender_for_bundle");

    remove_response_channel(bp.to_string().as_str());
    (vec![], false)
}
