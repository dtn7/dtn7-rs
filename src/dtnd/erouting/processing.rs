use crate::cla::ClaSender;
use crate::dtnd::erouting::{
    DroppedPeerPacket, EncounteredPeerPacket, IncomingBundlePacket,
    IncomingBundleWithoutPreviousNodePacket, Packet, PeerStatePacket, SendForBundlePacket,
    SendingFailedPacket,
};
use crate::RoutingNotifcation::{
    DroppedPeer, EncounteredPeer, IncomingBundle, IncomingBundleWithoutPreviousNode, SendingFailed,
};
use crate::{
    cla_names, lazy_static, peers_get_for_node, BundlePack, RoutingNotifcation, CONFIG, DTNCORE,
    PEERS,
};
use axum::extract::ws::{Message, WebSocket};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures_util::{future, pin_mut, StreamExt, TryStreamExt};
use log::{debug, info};
use std::sync::{Arc, Mutex};
use std::{thread, time};

type Tx = UnboundedSender<Message>;

struct Connection {
    tx: Tx,
    sfb_rx: UnboundedReceiver<Vec<ClaSender>>,
}

lazy_static! {
    static ref CONNECTION: Arc<Mutex<Option<Connection>>> = Arc::new(Mutex::new(None));
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
    let (sfb_tx, sfb_rx) = unbounded::<Vec<ClaSender>>();

    if CONNECTION.lock().unwrap().is_some() {
        info!("Websocket connection closed because external routing agent is already connected");
        if let Err(err) = ws.close().await {
            info!("Error while closing websocket: {}", err);
        }
        return;
    }

    *CONNECTION.lock().unwrap() = Some(Connection { tx, sfb_rx });

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
            Ok(packet) => match packet {
                Packet::SendForBundleResponsePacket(clas) => {
                    debug!(
                        "sender_for_bundle response: {}",
                        msg.to_text().unwrap().trim()
                    );
                    sfb_tx.unbounded_send(clas.clas);
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
        SendingFailed(a, b) => {
            packet = Packet::SendingFailedPacket(SendingFailedPacket {
                a: a.to_string(),
                b: b.to_string(),
            });
        }
        IncomingBundle(bndl) => {
            packet = Packet::IncomingBundlePacket(IncomingBundlePacket { bndl: bndl.clone() });
        }
        IncomingBundleWithoutPreviousNode(a, b) => {
            packet = Packet::IncomingBundleWithoutPreviousNodePacket(
                IncomingBundleWithoutPreviousNodePacket {
                    a: a.to_string(),
                    b: b.to_string(),
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

pub fn sender_for_bundle(bp: &BundlePack) -> (Vec<ClaSender>, bool) {
    let packet: Packet = Packet::SendForBundlePacket(SendForBundlePacket {
        clas: cla_names(),
        bp: bp.clone(),
    });
    send_packet(&packet);

    for _ in 0..20 {
        if let Some(con) = CONNECTION.lock().unwrap().as_mut() {
            if let Ok(Some(clas)) = con.sfb_rx.try_next() {
                return (clas, false);
            }
        } else {
            info!("No external routing! no sender_for_bundle possible");

            return (vec![], false);
        }

        thread::sleep(time::Duration::from_millis(200)); // TODO: Make timeout configurable or find better solution
    }

    info!("Timeout while waiting for sender_for_bundle");

    (vec![], false)
}
