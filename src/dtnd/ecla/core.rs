use crate::cla::external::ExternalConvergenceLayer;
use crate::cla::ConvergenceLayerAgent;
use crate::cla::RemoteAddr;
use crate::core::PeerType;
use crate::ipnd::services::ServiceBlock;
use crate::{cla_add, cla_remove, CONFIG};
use crate::{cla_names, DTNCLAS, DTNCORE};
use crate::{lazy_static, routing_notify, RoutingNotifcation};
use crate::{peers_add, DtnPeer};
use bp7::{Bundle, ByteBuffer, EndpointID};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};
use log::debug;
use log::error;
use log::info;
use serde::__private::TryFrom;
use serde::{Deserialize, Serialize};
use serde_json::Result;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::interval;
use tokio_tungstenite::tungstenite::Message;

type Tx = UnboundedSender<Message>;
type PeerMap = Arc<Mutex<HashMap<SocketAddr, Module>>>;

lazy_static! {
    static ref PEER_MAP: PeerMap = PeerMap::new(Mutex::new(HashMap::new()));
}

mod base64 {
    use base64::{decode, encode};
    use serde::{Deserialize, Serialize};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let base64 = encode(v);
        String::serialize(&base64, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        decode(base64.as_bytes()).map_err(|e| serde::de::Error::custom(e))
    }
}

// Represents in which state the Module WebSocket connection is.
enum ModuleState {
    // The Module has not signaled his name
    WaitingForIdent,
    // The Module has succesfully registered and is ready for messages
    Active,
}

// Represents the Module. A module has a connection state of the Websocket connection
// it's name (typically name of the used transmission protocol) and the tx which is the
// write stream to the underlying WebSocket.
struct Module {
    state: ModuleState,
    name: String,
    enable_beacon: bool,
    tx: Tx,
}

// The variant of Packets that can be send or received. The resulting JSON will have
// a field called type that encodes the selected variant.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Packet {
    RegisterPacket(RegisterPacket),
    Beacon(Beacon),
    ForwardDataPacket(ForwardDataPacket),
}

// Beacon is a device discovery packet. It can either be from the direct connection
// to the dtnd or received over the transmission layer of the ECLA client.
#[derive(Serialize, Deserialize)]
pub struct Beacon {
    eid: EndpointID,
    addr: String,
    #[serde(with = "base64")]
    service_block: Vec<u8>,
}

// Identification Packet that registers the Module Name.
#[derive(Serialize, Deserialize)]
struct RegisterPacket {
    name: String,
    enable_beacon: bool,
}

// Packet that forwards Bundle data
#[derive(Serialize, Deserialize)]
struct ForwardDataPacket {
    src: String,
    dst: String,
    #[serde(with = "base64")]
    data: Vec<u8>,
}

// Generates a beacon packet for the own dtnd instance.
pub fn generate_beacon() -> Beacon {
    let mut service_block = ServiceBlock::new();
    let mut beacon = Beacon {
        eid: (*CONFIG.lock()).host_eid.clone(),
        addr: "".to_string(),
        service_block: vec![],
    };

    // Get all available clas
    (*DTNCLAS.lock())
        .list
        .iter()
        .for_each(|cla| service_block.add_cla(&cla.name().to_string(), &Some(cla.port())));

    // Get all available services
    (*DTNCORE.lock())
        .service_list
        .iter()
        .for_each(|(tag, service)| {
            let payload = ServiceBlock::build_custom_service(*tag, service.as_str())
                .expect("Error while parsing Service to byte format");
            service_block.add_custom_service(*tag, &payload.1);
        });

    beacon.service_block = serde_cbor::to_vec(&service_block).unwrap();

    return beacon;
}

// Periodically advertises it's own node to the connected WebSocket clients.
async fn announcer() {
    let mut task = interval(crate::CONFIG.lock().announcement_interval);
    loop {
        debug!("waiting announcer");
        task.tick().await;
        debug!("running announcer");

        let mut pmap = PEER_MAP.lock().unwrap();
        pmap.retain(|_, value| {
            if !value.enable_beacon {
                return true;
            }

            let beacon: Packet = Packet::Beacon(generate_beacon());
            let data = serde_json::to_string(&beacon);
            value.tx.unbounded_send(Message::Text(data.unwrap())); // TODO: Handle error gracefully

            return true;
        });
    }
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
    PEER_MAP.lock().unwrap().insert(
        addr,
        Module {
            state: ModuleState::WaitingForIdent,
            name: "".to_string(),
            tx,
            enable_beacon: true,
        },
    );

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        info!(
            "Received a message from {}: {}",
            addr,
            msg.to_text().unwrap()
        );

        // Get own peer
        let mut pmap = PEER_MAP.lock().unwrap();

        let me_opt = pmap.get_mut(&addr);
        if me_opt.is_none() {
            return future::ok(());
        }

        let me = me_opt.unwrap();

        // Deserialize Packet
        let packet: Result<Packet> = serde_json::from_str(msg.to_text().unwrap());
        if packet.is_err() {
            return future::ok(());
        }

        match me.state {
            // If we are still in WaitingForIdent we only wait for RegisterPackets to register the Module name.
            ModuleState::WaitingForIdent => match packet.unwrap() {
                Packet::RegisterPacket(ident) => {
                    info!("Received RegisterPacket from {}: {}", addr, ident.name);

                    me.name = ident.name;
                    me.state = ModuleState::Active;

                    // TODO: check for wrong names
                    if !cla_names().contains(&me.name) {
                        info!("Adding CLA '{}'", me.name);

                        cla_add(ExternalConvergenceLayer::new(me.name.clone()).into());

                        // Send initial beacon of own
                        let initial_beacon: Packet = Packet::Beacon(generate_beacon());
                        let data = serde_json::to_string(&initial_beacon);
                        me.tx.unbounded_send(Message::Text(data.unwrap())); // TODO: Handle error gracefully
                    } else {
                        // TODO: send already registered message
                        // TODO: close connection
                    }
                }
                _ => {}
            },
            // If we are Active we wait for Beacon and ForwardDataPacket
            ModuleState::Active => match packet.unwrap() {
                // We got a new Bundle Packet that needs to be parsed and processed.
                Packet::ForwardDataPacket(fwd) => {
                    if let Ok(bndl) = Bundle::try_from(fwd.data) {
                        info!("Received bundle: {} from {}", bndl.id(), me.name);
                        {
                            tokio::spawn(async move {
                                if let Err(err) = crate::core::processing::receive(bndl).await {
                                    error!("Failed to process bundle: {}", err);
                                }
                            });
                        }
                    }
                }
                // We got a new Peer that is advertised through a Beacon Packet. The beacon packet
                // will typically be from the other side of the transmission Protocol that the connected
                // WebSocket client implements.
                Packet::Beacon(pdp) => {
                    debug!("Received beacon: {} {} {}", me.name, pdp.eid, pdp.addr);

                    let service_block: ServiceBlock =
                        serde_cbor::from_slice(pdp.service_block.as_slice()).unwrap();

                    peers_add(DtnPeer::new(
                        pdp.eid.clone(),
                        RemoteAddr::Str(pdp.addr),
                        PeerType::Dynamic,
                        None,
                        service_block.clas().clone(),
                        service_block.convert_services(),
                    ));

                    routing_notify(RoutingNotifcation::EncounteredPeer(&pdp.eid.clone()));
                }
                _ => {}
            },
        }

        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    info!("{} disconnected", &addr);
    cla_remove(PEER_MAP.lock().unwrap().get(&addr).unwrap().name.clone());
    PEER_MAP.lock().unwrap().remove(&addr);
}

pub fn scheduled_submission(name: &str, dest: &str, ready: &[ByteBuffer]) -> bool {
    debug!(
            "Scheduled submission External Convergence Layer for Destination with Module '{}' and Target '{}'",
            name, dest
        );

    let mut was_sent = false;
    let mut pmap = PEER_MAP.lock().unwrap();
    pmap.retain(|_, value| {
        if value.name == name {
            // Found the matching Module
            for b in ready {
                let packet: Packet = Packet::ForwardDataPacket(ForwardDataPacket {
                    dst: dest.to_string(),
                    src: "".to_string(), // Leave blank for now and let the Module set it to a protocol specific address on his side
                    data: b.to_vec(),
                });
                let data = serde_json::to_string(&packet);
                value.tx.unbounded_send(Message::Text(data.unwrap())); // TODO: Handle error gracefully
                was_sent = true;
            }
        }

        return true;
    });

    was_sent
}

pub fn start_ecla(port: u16) {
    debug!("Setup External Convergence Layer");

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

    tokio::spawn(announcer());
}
