use super::{Beacon, Connector, ForwardData, Packet};
use crate::cla::ecla::tcp::TCPConnector;
use crate::cla::ecla::ws::WebsocketConnector;
use crate::cla::ecla::{ConnectorEnum, Error, Registered};
use crate::cla::external::ExternalConvergenceLayer;
use crate::cla::{ConvergenceLayerAgent, TransferResult};
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

/// Specifies the maximum length for a name of ECLA modules.
const ECLA_NAME_MAX_LEN: usize = 64;

type ModuleMap = Arc<Mutex<HashMap<String, Module>>>;
type LayerMap = Arc<Mutex<HashMap<String, ConnectorEnum>>>;

lazy_static! {
    /// Tracks the registered connectors over which clients can connect to dtnd (e.g. WebSocket or TCP).
    static ref CONNECTORS_MAP: LayerMap = LayerMap::new(Mutex::new(HashMap::new()));
    /// Tracks the registered modules that are connected over a connector.
    static ref MODULE_MAP: ModuleMap = ModuleMap::new(Mutex::new(HashMap::new()));
}

/// Represents in which state the Module connection is.
enum ModuleState {
    /// The Module has not signaled his name.
    WaitingForIdent,
    /// The Module has successfully registered and is ready for messages.
    Active,
}

/// Represents the Module. A module holds its connection state, its name (typically the name of the used transmission protocol),
/// the connector over which it's connected and if the optional service discovery via periodically sent beacons is enabled.
struct Module {
    state: ModuleState,
    /// Name of the Module should be the externally implemented CLA name (e.g. BLE, MTCP, LoRa, ...)
    name: String,
    /// Name of the connector which the model is connected through (e.g. WebSocket, TCP, ...)
    connector: String,
    /// Specifies if the Module requested the optional service discovery to be enabled.
    enable_beacon: bool,
}

/// Generates a beacon packet that contains advertisement information about this dtnd instance.
pub fn generate_beacon() -> Beacon {
    let mut service_block = ServiceBlock::new();
    let mut beacon = Beacon {
        eid: CONFIG.lock().host_eid.clone(),
        addr: "".to_string(),
        service_block: vec![],
    };

    // Get all available clas
    (*CLAS.lock())
        .iter()
        .for_each(|cla| service_block.add_cla(cla.name(), &Some(cla.port())));

    // Get all available services
    DTNCORE
        .lock()
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

/// Periodically advertises its own node to the clients.
async fn announcer() {
    let mut task = interval(crate::CONFIG.lock().announcement_interval);
    loop {
        task.tick().await;

        MODULE_MAP.lock().unwrap().iter().for_each(|(addr, value)| {
            if !value.enable_beacon {
                return;
            }

            if let Some(connector) = CONNECTORS_MAP
                .lock()
                .unwrap()
                .get_mut(value.connector.as_str())
            {
                debug!("Sending Beacon to {} ({})", addr, value.connector);
                connector.send_packet(addr, &Packet::Beacon(generate_beacon()));
            }
        });
    }
}

/// Handles packets from a connector
pub fn handle_packet(connector_name: String, addr: String, packet: Packet) {
    let mut module_map = MODULE_MAP.lock().unwrap();

    // Check if the module exists.
    let mod_opt = module_map.get_mut(&addr);
    if mod_opt.is_none() {
        return;
    }

    // Check if the corresponding layer exists without holding the lock for the whole function.
    {
        let connectors_map = CONNECTORS_MAP.lock().unwrap();
        if connectors_map.get(&connector_name).is_none() {
            return;
        }
    }

    let me = mod_opt.unwrap();
    match me.state {
        // If we are still in WaitingForIdent we only wait for RegisterPackets to register the Module name.
        ModuleState::WaitingForIdent => {
            if let Packet::Register(ident) = packet {
                info!(
                    "Received RegisterPacket from {} ({}): {}",
                    addr, connector_name, ident.name
                );

                let mut connectors_map = CONNECTORS_MAP.lock().unwrap();
                let connector = connectors_map.get_mut(&connector_name).unwrap();

                if ident.name.is_empty() || ident.name.len() > ECLA_NAME_MAX_LEN {
                    error!("Rejected ECLA because name was empty or too long");

                    connector.send_packet(
                        addr.as_str(),
                        &Packet::Error(Error {
                            reason: "invalid name".to_string(),
                        }),
                    );
                    connector.close(addr.as_str());
                } else if !cla_names().contains(&ident.name) {
                    me.name = ident.name;
                    me.state = ModuleState::Active;

                    info!("Adding CLA '{}'", me.name);

                    let mut settings: HashMap<String, String> = HashMap::new();
                    settings.insert("name".to_string(), me.name.clone());

                    if let Some(port) = ident.port {
                        settings.insert("port".to_string(), port.to_string());
                    }

                    cla_add(ExternalConvergenceLayer::new(Option::Some(&settings)).into());

                    // Send registered packet
                    let eid = CONFIG.lock().host_eid.clone();
                    let nodeid = CONFIG.lock().nodeid.clone();
                    connector.send_packet(
                        addr.as_str(),
                        &Packet::Registered(Registered { eid, nodeid }),
                    );

                    // Send initial beacon
                    if me.enable_beacon {
                        connector.send_packet(addr.as_str(), &Packet::Beacon(generate_beacon()));
                    }
                } else {
                    error!("Rejected ECLA because '{}' CLA is already present", me.name);

                    connector.send_packet(
                        addr.as_str(),
                        &Packet::Error(Error {
                            reason: "already registered".to_string(),
                        }),
                    );
                    connector.close(addr.as_str());
                }
            }
        }
        // If we are Active we wait for Beacon and ForwardDataPacket
        ModuleState::Active => match packet {
            // We got a new Bundle Packet that needs to be parsed and processed.
            Packet::ForwardData(fwd) => {
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
            // client implements.
            Packet::Beacon(pdp) => {
                info!("Received beacon: {} {} {}", me.name, pdp.eid, pdp.addr);

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

                let cmd_channel = DTNCORE.lock().routing_agent.channel();
                if let Err(err) = cmd_channel.try_send(RoutingCmd::Notify(
                    RoutingNotifcation::EncounteredPeer(pdp.eid),
                )) {
                    error!("Failed to send encountered peer notification: {}", err);
                }
            }
            _ => {}
        },
    }
}

/// When a module connects in a connector, this function should be called.
/// It will initialize the information about the new module.
pub fn handle_connect(connector_name: String, from: String) {
    MODULE_MAP.lock().unwrap().insert(
        from,
        Module {
            state: ModuleState::WaitingForIdent,
            name: "".to_string(),
            connector: connector_name,
            enable_beacon: true,
        },
    );
}

/// When a module disconnects in a connector this function should be called. It will remove the
/// client from the internal module registry and remove the CLA if the module was already fully registered.
pub fn handle_disconnect(addr: String) {
    info!("{} disconnected", &addr);

    if let Some(module) = MODULE_MAP.lock().unwrap().get(&addr) {
        if let ModuleState::Active = module.state {
            cla_remove(module.name.clone());
        }
    }

    MODULE_MAP.lock().unwrap().remove(&addr);
}

/// Will schedule a submission to a module by name
pub fn scheduled_submission(name: String, dest: String, ready: &ByteBuffer) -> TransferResult {
    debug!(
            "Scheduled submission External Convergence Layer for Destination with Module '{}' and Target '{}'",
            name, dest
        );

    let mut was_sent = TransferResult::Failure;
    let mut connectors_map = CONNECTORS_MAP.lock().unwrap();
    let module_map = MODULE_MAP.lock().unwrap();
    module_map.iter().for_each(|(addr, value)| {
        if value.name == name {
            if let Ok(bndl) = Bundle::try_from(ready.as_slice()) {
                let packet: Packet = Packet::ForwardData(ForwardData {
                    dst: dest.to_string(),
                    src: "".to_string(), // Leave blank for now and let the Module set it to a protocol-specific address on its side
                    bundle_id: bndl.id(),
                    data: ready.to_vec(),
                });

                if let Some(connector) = connectors_map.get_mut(value.connector.as_str()) {
                    connector.send_packet(addr, &packet);
                    was_sent = TransferResult::Successful;
                }
            }
        }
    });

    was_sent
}

/// Adds a connector to the registered connectors.
pub fn add_connector(connector: ConnectorEnum) {
    CONNECTORS_MAP
        .lock()
        .unwrap()
        .insert(connector.name().to_string(), connector);
}

/// Starts the websocket connector by default and the tcp connector when the tcp_port > 0.
pub async fn start_ecla(tcp_port: u16) {
    debug!("Setup External Convergence Layer");

    // Create the websocket connector
    add_connector(WebsocketConnector::new().into());

    // Create the tcp connector
    if tcp_port > 0 {
        let mut tcp_layer = TCPConnector::new(tcp_port);
        tcp_layer.setup().await;
        add_connector(tcp_layer.into());
    }

    tokio::spawn(announcer());
}
