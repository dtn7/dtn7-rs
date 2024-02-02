pub mod cla;
pub mod client;
pub mod core;
pub mod dtnconfig;
pub mod dtnd;
pub mod ipnd;
pub mod routing;

use crate::cla::CLAsAvailable;
use crate::core::bundlepack::BundlePack;
use crate::core::store::{BundleStore, InMemoryBundleStore};
use crate::core::DtnStatistics;
use crate::routing::{RoutingAgent, RoutingCmd};
use bp7::{Bundle, EndpointID};
use cla::{CLAEnum, ClaSenderTask};
pub use dtnconfig::DtnConfig;
use log::{debug, error, info};

pub use crate::core::{DtnCore, DtnPeer};
pub use crate::routing::RoutingNotifcation;

use crate::cla::ConvergenceLayerAgent;
use crate::core::bundlepack::Constraint;
use crate::core::peer::PeerAddress;
use crate::core::store::BundleStoresEnum;
use anyhow::{bail, Context, Result};
use lazy_static::*;
use parking_lot::Mutex;
use std::collections::{BTreeMap, HashMap};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

lazy_static! {
    pub static ref CONFIG: Mutex<DtnConfig> = Mutex::new(DtnConfig::new());
    pub static ref DTNCORE: Mutex<DtnCore> = Mutex::new(DtnCore::new());
    pub static ref PEERS: Mutex<BTreeMap<String, DtnPeer>> = Mutex::new(BTreeMap::new());
    pub static ref STATS: Mutex<DtnStatistics> = Mutex::new(DtnStatistics::new());
    pub static ref SENDERTASK: Mutex<Option<Sender<Bundle>>> = Mutex::new(None);
    pub static ref STORE: Mutex<BundleStoresEnum> = Mutex::new(InMemoryBundleStore::new().into());
    pub static ref CLAS: Mutex<Vec<CLAEnum>> = Mutex::new(Vec::new());
}

pub fn cla_add(cla: CLAEnum) {
    (*CLAS.lock()).push(cla);
}
pub fn cla_remove(name: String) {
    (*CLAS.lock()).retain(|value| {
        return value.name() != name;
    })
}
pub fn cla_is_external(name: String) -> bool {
    return (*CLAS.lock()).iter().any(|p| match p {
        CLAEnum::ExternalConvergenceLayer(e) => {
            return e.name() == name;
        }
        _ => false,
    });
}
pub fn cla_parse(name: &str) -> CLAsAvailable {
    if cla_is_external(name.to_string()) {
        return CLAsAvailable::ExternalConvergenceLayer;
    }

    name.parse::<CLAsAvailable>().unwrap()
}
pub fn cla_settings(name: String) -> Option<HashMap<String, String>> {
    let res: Vec<Option<HashMap<String, String>>> = (*CLAS.lock())
        .iter()
        .filter(|p| {
            return p.name() == name;
        })
        .map(|p| p.local_settings())
        .collect();

    if res.is_empty() || res[0].is_none() {
        return None;
    }

    return Some(res[0].as_ref().unwrap().clone());
}
pub fn cla_names() -> Vec<String> {
    let names: Vec<String> = (*CLAS.lock())
        .iter()
        .map(|val| {
            return String::from(val.name());
        })
        .collect();

    names
}
pub fn service_add(tag: u8, service: String) {
    DTNCORE.lock().service_list.insert(tag, service);
}
pub fn add_discovery_destination(destination: &str) {
    CONFIG
        .lock()
        .discovery_destinations
        .insert(destination.to_string(), 0);
}

pub fn reset_sequence(destination: &str) {
    if let Some(sequence) = CONFIG.lock().discovery_destinations.get_mut(destination) {
        *sequence = 0;
    }
}
pub fn get_sequence(destination: &str) -> u32 {
    if let Some(sequence) = CONFIG.lock().discovery_destinations.get(destination) {
        *sequence
    } else {
        0
    }
}
/// adds a new peer to the DTN core
/// return true if peer was seen first time
/// return false if peer was already known
pub fn peers_add(peer: DtnPeer) -> bool {
    (*PEERS.lock())
        .insert(peer.eid.node().unwrap(), peer)
        .is_none()
}
pub fn peers_count() -> usize {
    (*PEERS.lock()).len()
}
pub fn peers_clear() {
    (*PEERS.lock()).clear();
}
pub fn peers_known(peer: &str) -> bool {
    (*PEERS.lock()).contains_key(peer)
}
pub fn peers_touch(peer: &str) -> Result<()> {
    (*PEERS.lock())
        .get_mut(peer)
        .context("no such peer")?
        .touch();
    Ok(())
}
pub fn peers_get_for_node(eid: &EndpointID) -> Option<DtnPeer> {
    for (_, p) in (*PEERS.lock()).iter() {
        if p.node_name() == eid.node().unwrap_or_default() {
            return Some(p.clone());
        }
    }
    None
}
pub fn is_local_node_id(eid: &EndpointID) -> bool {
    eid.node_id() == CONFIG.lock().host_eid.node_id()
}
pub fn peers_cla_for_node(eid: &EndpointID) -> Option<ClaSenderTask> {
    if let Some(peer) = peers_get_for_node(eid) {
        return peer.first_cla();
    }
    None
}
pub fn peer_find_by_remote(addr: &PeerAddress) -> Option<String> {
    for (_, p) in (*PEERS.lock()).iter() {
        if p.addr() == addr {
            return Some(p.node_name());
        }
    }
    None
}

pub fn store_push_bundle(bndl: &Bundle) -> Result<()> {
    (*STORE.lock()).push(bndl)
}

pub fn store_add_bundle_if_unknown(bndl: &Bundle) -> Result<bool> {
    let store = &mut (*STORE.lock());
    if !store.has_item(bndl.id().as_str()) {
        store.push(bndl)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn store_remove(bid: &str) -> Result<()> {
    info!("Removing bundle {}", bid);
    if let Err(err) = (*STORE.lock()).remove(bid) {
        error!("store_remove: {}", err);
        return Err(err);
    }
    Ok(())
}

pub fn store_update_metadata(bp: &BundlePack) -> Result<()> {
    (*STORE.lock()).update_metadata(bp)
}
pub fn store_has_item(bid: &str) -> bool {
    (*STORE.lock()).has_item(bid)
}
pub fn store_get_bundle(bpid: &str) -> Option<Bundle> {
    (*STORE.lock()).get_bundle(bpid)
}
pub fn store_get_metadata(bpid: &str) -> Option<BundlePack> {
    (*STORE.lock()).get_metadata(bpid)
}

pub fn store_delete_expired() {
    let all_bids = (*STORE.lock()).bundles();

    let all_but_deleted: Vec<&BundlePack> = all_bids
        .iter()
        .filter(|bp| !bp.has_constraint(Constraint::Deleted))
        .collect();

    for meta in all_but_deleted {
        if meta.has_expired() {
            debug!("Bundle {} is too old, deleting it", meta.id);
            if store_remove(&meta.id).is_err() {
                error!("Error while deleting expired bundle {}", meta.id);
            }
        }
    }
}

pub async fn routing_notify(notification: RoutingNotifcation) -> Result<()> {
    let chan = DTNCORE.lock().routing_agent.channel();
    if let Err(err) = chan.send(RoutingCmd::Notify(notification)).await {
        bail!("Error while sending notification: {}", err);
    }
    Ok(())
}

pub async fn routing_sender_for_bundle(bp: BundlePack) -> Result<(Vec<ClaSenderTask>, bool)> {
    let (reply_tx, reply_rx) = oneshot::channel();

    let cmd_channel = DTNCORE.lock().routing_agent.channel();
    if let Err(err) = cmd_channel
        .send(RoutingCmd::SenderForBundle(bp, reply_tx))
        .await
    {
        bail!("Error while sending command to routing agent: {}", err);
    }

    let res = reply_rx.await;
    if let Err(err) = res {
        bail!("Error while waiting for SenderForBundle reply: {}", err);
    }

    Ok(res.unwrap())
}
