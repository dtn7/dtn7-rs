use crate::cla::ConvergenceLayerAgent;
use crate::core::{DtnPeer, PeerType};
use crate::routing::RoutingNotifcation;
use crate::{peers_add, peers_remove, peers_touch, routing_notify, CLAS, CONFIG, DTNCORE};
use anyhow::Result;
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::net::IpAddr;
use tokio::time::interval;

const MDNS_SERVICE_TYPE: &str = "_dtn._udp.local.";

pub async fn spawn_mdns_discovery() -> Result<()> {
    use mdns_sd::ServiceDaemon;

    let config = CONFIG.lock();
    let nodeid = config.nodeid.clone();
    let eid = config.host_eid.clone();
    let interval_duration = config.announcement_interval;
    drop(config);

    info!("Starting mDNS discovery for service type: {}", MDNS_SERVICE_TYPE);

    // Create mDNS daemon
    let mdns = ServiceDaemon::new().expect("Failed to create mDNS daemon");

    // Build and register our service
    let service_info = build_mdns_service_info(&nodeid, &eid);
    mdns.register(service_info)
        .expect("Failed to register mDNS service");

    info!("Registered mDNS service type: {} (Node: {})", MDNS_SERVICE_TYPE, nodeid);

    // Browse for other DTN nodes
    let receiver = mdns.browse(MDNS_SERVICE_TYPE)
        .expect("Failed to browse for mDNS services");

    // Spawn listener task
    tokio::spawn(async move {
        loop {
            match receiver.recv_async().await {
                Ok(event) => {
                    if let Err(e) = handle_mdns_event(event).await {
                        error!("Error handling mDNS event: {}", e);
                    }
                }
                Err(e) => {
                    error!("mDNS receiver error: {}", e);
                    break;
                }
            }
        }
    });

    // Spawn periodic refresh task (update service info when CLAs change)
    tokio::spawn(async move {
        let mut task = interval(interval_duration);
        loop {
            task.tick().await;

            let config = CONFIG.lock();
            let nodeid = config.nodeid.clone();
            let eid = config.host_eid.clone();
            drop(config);

            // Re-register service with updated info (including fresh timestamp)
            let service_info = build_mdns_service_info(&nodeid, &eid);
            if let Err(e) = mdns.register(service_info) {
                warn!("Failed to refresh mDNS service: {}", e);
            } else {
                debug!("Refreshed mDNS service registration with updated timestamp");
            }
        }
    });

    Ok(())
}

fn build_mdns_service_info(nodeid: &str, eid: &bp7::EndpointID) -> mdns_sd::ServiceInfo {
    use mdns_sd::ServiceInfo;

    let mut properties: Vec<(&str, String)> = Vec::new();

    // Add endpoint ID
    let eid_str = eid.to_string();
    properties.push(("eid", eid_str));

    // Add timestamp to force mDNS to broadcast updates even when other data hasn't changed
    // This ensures remote nodes receive periodic ServiceResolved events for keepalive
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    properties.push(("ts", timestamp.to_string()));

    // Add available CLAs with their ports
    let clas = CLAS.lock();
    let cla_props: Vec<(String, String)> = clas.iter()
        .enumerate()
        .map(|(idx, cla)| {
            (format!("cla{}", idx), format!("{}:{}", cla.name(), cla.port()))
        })
        .collect();
    drop(clas);

    for (key, val) in cla_props {
        properties.push((Box::leak(key.into_boxed_str()), val));
    }

    // Add custom services
    let services = DTNCORE.lock().service_list.clone();
    let svc_props: Vec<(String, String)> = services.iter()
        .map(|(tag, payload)| (format!("svc{}", tag), payload.clone()))
        .collect();

    for (key, val) in svc_props {
        properties.push((Box::leak(key.into_boxed_str()), val));
    }

    // Convert to format mdns-sd expects
    let props_ref: Vec<(&str, &str)> = properties.iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    // Use a dummy port (actual CLAs have their own ports)
    // The hostname will be <nodeid>.local
    ServiceInfo::new(
        MDNS_SERVICE_TYPE,
        nodeid,
        &format!("{}.local.", nodeid),
        "",
        0, // Port not used for DTN discovery
        &props_ref[..],
    )
    .expect("Failed to create ServiceInfo")
    .enable_addr_auto()
}

async fn handle_mdns_event(event: mdns_sd::ServiceEvent) -> Result<()> {
    use mdns_sd::ServiceEvent;

    match event {
        ServiceEvent::ServiceResolved(info) => {
            trace!("mDNS service resolved: {:?}", info);
            handle_mdns_peer_discovered(info).await?;
        }
        ServiceEvent::ServiceRemoved(_, fullname) => {
            debug!("mDNS service removed: {}", fullname);
            handle_mdns_peer_removed(&fullname).await?;
        }
        ServiceEvent::ServiceFound(_, fullname) => {
            // Touch peer to keep it alive (mDNS announces service existence)
            // Note: This event fires before ServiceResolved, so the peer might not exist yet
            debug!("mDNS ServiceFound event (keepalive): {}", fullname);
            if let Some(instance_name) = fullname.split('.').next() {
                // Extract node name from EID (e.g., "dtn://node1/" -> "node1")
                let node_name = if instance_name.starts_with("dtn://") && instance_name.ends_with('/') {
                    &instance_name[6..instance_name.len()-1]
                } else {
                    instance_name
                };

                // Silently ignore if peer doesn't exist yet - ServiceResolved will add it
                if let Ok(_) = peers_touch(node_name) {
                    debug!("Successfully touched peer: {}", node_name);
                }
            }
        }
        ServiceEvent::SearchStarted(_) => {
            debug!("mDNS search started");
        }
        ServiceEvent::SearchStopped(_) => {
            warn!("mDNS search stopped");
        }
    }

    Ok(())
}

async fn handle_mdns_peer_discovered(info: mdns_sd::ServiceInfo) -> Result<()> {
    // Extract EID from TXT records
    let eid_str = info.get_property_val_str("eid")
        .ok_or_else(|| anyhow::anyhow!("No EID in mDNS service info"))?;

    let eid: bp7::EndpointID = bp7::eid::EndpointID::try_from(eid_str.as_ref())
        .map_err(|e| anyhow::anyhow!("Invalid EID in mDNS service: {:?}", e))?;

    // Don't add ourselves
    let our_eid = CONFIG.lock().host_eid.clone();
    if eid == our_eid {
        debug!("Ignoring mDNS service from ourselves: {}", eid);
        return Ok(());
    }

    // Extract CLAs from TXT records
    let mut clas = Vec::new();
    for prop in info.get_properties().iter() {
        let key = prop.key();
        if key.starts_with("cla") {
            if let Some(val) = info.get_property_val_str(key) {
                if let Some((name, port_str)) = val.split_once(':') {
                    let port = port_str.parse().ok();
                    clas.push((name.to_string(), port));
                }
            }
        }
    }

    // Extract custom services
    let mut services = HashMap::new();
    for prop in info.get_properties().iter() {
        let key = prop.key();
        if key.starts_with("svc") {
            if let Ok(tag) = key.strip_prefix("svc").unwrap_or("").parse::<u8>() {
                if let Some(val) = info.get_property_val_str(key) {
                    services.insert(tag, val.to_string());
                }
            }
        }
    }

    // Get IP addresses from mDNS response
    let addresses: Vec<IpAddr> = info.get_addresses().iter().cloned().collect();

    if addresses.is_empty() {
        return Err(anyhow::anyhow!("No addresses in mDNS service info"));
    }

    // Log all discovered addresses
    let addr_str = addresses.iter()
        .map(|a| a.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Prefer IPv4 over IPv6 link-local addresses for better reachability
    let addr = addresses.iter()
        .find(|ip| ip.is_ipv4())
        .or_else(|| addresses.iter().find(|ip| !ip.to_string().starts_with("fe80::")))
        .or_else(|| addresses.first())
        .ok_or_else(|| anyhow::anyhow!("No valid address in mDNS service info"))?
        .clone();

    debug!("Selected address {} from [{}]", addr, addr_str);

    // Create peer
    let peer = DtnPeer::new(
        eid.clone(),
        addr.into(),
        PeerType::Dynamic,
        None, // mDNS handles timing
        clas,
        services,
    );

    let is_new = peers_add(peer);
    if let Err(e) = peers_touch(eid.node().unwrap().as_ref()) {
        warn!("Failed to touch peer after add: {}", e);
    }

    if is_new {
        info!("New peer discovered via mDNS: {}", eid);
    } else {
        debug!("Updated existing peer via mDNS: {} @ [{}]", eid, addr_str);
    }

    // Notify routing
    routing_notify(RoutingNotifcation::EncounteredPeer(eid)).await?;

    Ok(())
}

async fn handle_mdns_peer_removed(fullname: &str) -> Result<()> {
    // Extract instance name (nodeid) from fullname
    let instance = fullname.split('.').next()
        .ok_or_else(|| anyhow::anyhow!("Invalid mDNS fullname"))?;

    info!("Removing peer discovered via mDNS: {}", instance);
    peers_remove(instance);

    Ok(())
}
