use super::{Beacon, ForwardDataPacket, Packet, TransportLayer};
use crate::cla::external::ExternalConvergenceLayer;
use crate::cla::ConvergenceLayerAgent;
use crate::cla::RemoteAddr;
use crate::core::PeerType;
use crate::dtnd::ecla::tcp::TCPTransportLayer;
use crate::dtnd::ecla::ws::WebsocketTransportLayer;
use crate::dtnd::ecla::TransportLayerEnum;
use crate::ipnd::services::ServiceBlock;
use crate::{cla_add, cla_remove, CONFIG};
use crate::{cla_names, DTNCLAS, DTNCORE};
use crate::{lazy_static, routing_notify, RoutingNotifcation};
use crate::{peers_add, DtnPeer};
use bp7::{Bundle, ByteBuffer};
use log::debug;
use log::error;
use log::info;
use serde::__private::TryFrom;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::time::interval;

type ModuleMap = Arc<Mutex<HashMap<String, Module>>>;
type LayerMap = Arc<Mutex<HashMap<String, TransportLayerEnum>>>;

lazy_static! {
    static ref MODULE_MAP: ModuleMap = ModuleMap::new(Mutex::new(HashMap::new()));
    static ref LAYER_MAP: LayerMap = LayerMap::new(Mutex::new(HashMap::new()));
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
    layer: String,
    enable_beacon: bool,
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

    beacon
}

// Periodically advertises it's own node to the connected WebSocket clients.
async fn announcer() {
    let mut task = interval(crate::CONFIG.lock().announcement_interval);
    loop {
        debug!("waiting announcer");
        task.tick().await;
        debug!("running announcer");

        let mut mmap = MODULE_MAP.lock().unwrap();
        let mut lmap = LAYER_MAP.lock().unwrap();
        mmap.retain(|addr, value| {
            if !value.enable_beacon {
                return true;
            }

            if let Some(layer) = lmap.get_mut(value.layer.as_str()) {
                debug!("Sending Beacon to {} ({})", addr, value.layer);
                layer.send_packet(addr, &Packet::Beacon(generate_beacon()));
            }

            true
        });
    }
}

pub fn handle_packet(layer_name: String, addr: String, packet: Packet) {
    // Get own peer
    let mut mmap = MODULE_MAP.lock().unwrap();
    let mut lmap = LAYER_MAP.lock().unwrap();

    let mod_opt = mmap.get_mut(&addr);
    let layer_opt = lmap.get_mut(&layer_name);
    if mod_opt.is_none() || layer_opt.is_none() {
        return;
    }

    let layer = layer_opt.unwrap();
    let me = mod_opt.unwrap();
    match me.state {
        // If we are still in WaitingForIdent we only wait for RegisterPackets to register the Module name.
        ModuleState::WaitingForIdent => {
            if let Packet::RegisterPacket(ident) = packet {
                info!(
                    "Received RegisterPacket from {} ({}): {}",
                    addr, layer_name, ident.name
                );

                me.name = ident.name;
                me.state = ModuleState::Active;

                // TODO: check for wrong names
                if !cla_names().contains(&me.name) {
                    info!("Adding CLA '{}'", me.name);

                    cla_add(ExternalConvergenceLayer::new(me.name.clone()).into());

                    // Send initial beacon of own
                    layer.send_packet(addr.as_str(), &Packet::Beacon(generate_beacon()));
                } else {
                    // TODO: send already registered message
                    // TODO: close connection
                }
            }
        }
        // If we are Active we wait for Beacon and ForwardDataPacket
        ModuleState::Active => match packet {
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
}

pub fn handle_connect(layer_name: String, from: String) {
    MODULE_MAP.lock().unwrap().insert(
        from,
        Module {
            state: ModuleState::WaitingForIdent,
            name: "".to_string(),
            layer: layer_name.to_string(),
            enable_beacon: true,
        },
    );
}

pub fn handle_disconnect(addr: String) {
    info!("{} disconnected", &addr);
    cla_remove(MODULE_MAP.lock().unwrap().get(&addr).unwrap().name.clone());
    MODULE_MAP.lock().unwrap().remove(&addr);
}

pub fn scheduled_submission(name: &str, dest: &str, ready: &[ByteBuffer]) -> bool {
    debug!(
            "Scheduled submission External Convergence Layer for Destination with Module '{}' and Target '{}'",
            name, dest
        );

    let mut was_sent = false;
    let mut mmap = MODULE_MAP.lock().unwrap();
    let mut lmap = LAYER_MAP.lock().unwrap();
    mmap.retain(|addr, value| {
        if value.name == name {
            // Found the matching Module
            for b in ready {
                let packet: Packet = Packet::ForwardDataPacket(ForwardDataPacket {
                    dst: dest.to_string(),
                    src: "".to_string(), // Leave blank for now and let the Module set it to a protocol specific address on his side
                    data: b.to_vec(),
                });

                if let Some(layer) = lmap.get_mut(value.layer.as_str()) {
                    layer.send_packet(addr, &packet);
                    was_sent = true;
                }
            }
        }

        true
    });

    was_sent
}

pub fn add_layer(layer: TransportLayerEnum) {
    LAYER_MAP
        .lock()
        .unwrap()
        .insert(layer.name().to_string(), layer);
}

pub async fn start_ecla(port: u16) {
    debug!("Setup External Convergence Layer");

    // Create the WebSocket server here for now
    let mut ws_layer = WebsocketTransportLayer::new();
    ws_layer.setup().await;
    add_layer(ws_layer.into());

    // Create the TCP server here for now
    let mut tcp_layer = TCPTransportLayer::new(port + 10);
    tcp_layer.setup().await;
    add_layer(tcp_layer.into());

    tokio::spawn(announcer());
}
