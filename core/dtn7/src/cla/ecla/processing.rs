use super::{Beacon, ForwardData, Packet, TransportLayer};
use crate::cla::ecla::tcp::TCPTransportLayer;
use crate::cla::ecla::ws::WebsocketTransportLayer;
use crate::cla::ecla::{Error, Registered, TransportLayerEnum};
use crate::cla::external::ExternalConvergenceLayer;
use crate::cla::ConvergenceLayerAgent;
use crate::core::PeerType;
use crate::ipnd::services::ServiceBlock;
use crate::routing::RoutingAgent;
use crate::{cla_add, cla_remove, PeerAddress, RoutingCmd, CONFIG};
use crate::{cla_names, CLAS, DTNCORE};
use crate::{lazy_static, RoutingNotifcation};
use crate::{peers_add, DtnPeer};
use bp7::{Bundle, ByteBuffer};
use log::{debug, error, info};
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

/// Represents in which state the Module WebSocket connection is.
enum ModuleState {
    /// The Module has not signaled his name
    WaitingForIdent,
    /// The Module has succesfully registered and is ready for messages
    Active,
}

/// Represents the Module. A module has a connection state of the Websocket connection
/// it's name (typically name of the used transmission protocol) and the tx which is the
/// write stream to the underlying WebSocket.
struct Module {
    state: ModuleState,
    name: String,
    layer: String,
    enable_beacon: bool,
}

/// Generates a beacon packet for the own dtnd instance.
pub fn generate_beacon() -> Beacon {
    let mut service_block = ServiceBlock::new();
    let mut beacon = Beacon {
        eid: (*CONFIG.lock()).host_eid.clone(),
        addr: "".to_string(),
        service_block: vec![],
    };

    // Get all available clas
    (*CLAS.lock())
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

/// Periodically advertises it's own node to the connected WebSocket clients.
async fn announcer() {
    let mut task = interval(crate::CONFIG.lock().announcement_interval);
    loop {
        task.tick().await;

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
            if let Packet::Register(ident) = packet {
                debug!(
                    "Received RegisterPacket from {} ({}): {}",
                    addr, layer_name, ident.name
                );

                me.name = ident.name;
                me.state = ModuleState::Active;

                // TODO: check for wrong names
                if !cla_names().contains(&me.name) {
                    info!("Adding CLA '{}'", me.name);

                    let mut settings: HashMap<String, String> = HashMap::new();
                    settings.insert("name".to_string(), me.name.clone());

                    if let Some(port) = ident.port {
                        settings.insert("port".to_string(), port.to_string());
                    }

                    cla_add(ExternalConvergenceLayer::new(Option::Some(&settings)).into());

                    // Send registered packet
                    let eid = (*CONFIG.lock()).host_eid.clone();
                    let nodeid = (*CONFIG.lock()).nodeid.clone();
                    layer.send_packet(
                        addr.as_str(),
                        &Packet::Registered(Registered { eid, nodeid }),
                    );

                    // Send initial beacon
                    if me.enable_beacon {
                        layer.send_packet(addr.as_str(), &Packet::Beacon(generate_beacon()));
                    }
                } else {
                    error!("Rejected ECLA because '{}' CLA is already present", me.name);

                    layer.send_packet(
                        addr.as_str(),
                        &Packet::Error(Error {
                            reason: "already registered".to_string(),
                        }),
                    );
                    layer.close(addr.as_str());
                }
            }
        }
        // If we are Active we wait for Beacon and ForwardDataPacket
        ModuleState::Active => match packet {
            // We got a new Bundle Packet that needs to be parsed and processed.
            Packet::ForwardData(fwd) => {
                if let Ok(bndl) = Bundle::try_from(fwd.data) {
                    debug!("Received bundle: {} from {}", bndl.id(), me.name);
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
                    PeerAddress::Generic(pdp.addr),
                    PeerType::Dynamic,
                    None,
                    service_block.clas().clone(),
                    service_block.convert_services(),
                ));

                let cmd_channel = (*DTNCORE.lock()).routing_agent.channel();
                if let Err(err) = cmd_channel.try_send(RoutingCmd::Notify(
                    RoutingNotifcation::EncounteredPeer(pdp.eid.clone()),
                )) {
                    error!("Failed to add peer: {}", err);
                }
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

pub fn scheduled_submission(name: String, dest: String, ready: &ByteBuffer) -> bool {
    debug!(
            "Scheduled submission External Convergence Layer for Destination with Module '{}' and Target '{}'",
            name, dest
        );

    let mut was_sent = false;
    let mut mmap = MODULE_MAP.lock().unwrap();
    let mut lmap = LAYER_MAP.lock().unwrap();
    mmap.retain(|addr, value| {
        if value.name == name {
            if let Ok(bndl) = Bundle::try_from(ready.as_slice()) {
                let packet: Packet = Packet::ForwardData(ForwardData {
                    dst: dest.to_string(),
                    src: "".to_string(), // Leave blank for now and let the Module set it to a protocol specific address on his side
                    bundle_id: bndl.id(),
                    data: ready.to_vec(),
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

pub async fn start_ecla(tcpport: u16) {
    debug!("Setup External Convergence Layer");

    // Create the WS Transport Layer
    add_layer(WebsocketTransportLayer::new().into());

    // Create the TCP Transport Layer
    if tcpport > 0 {
        let mut tcp_layer = TCPTransportLayer::new(tcpport);
        tcp_layer.setup().await;
        add_layer(tcp_layer.into());
    }

    tokio::spawn(announcer());
}
