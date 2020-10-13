pub mod cla;
pub mod core;
pub mod dtnconfig;
pub mod dtnd;
pub mod ipnd;
pub mod routing;

use crate::cla::ConvergenceLayerAgent;
use crate::core::bundlepack::BundlePack;
use crate::core::store::{BundleStore, InMemoryBundleStore};
use crate::core::DtnStatistics;
use bp7::{Bundle, EndpointID};
pub use dtnconfig::DtnConfig;

pub use crate::core::{DtnCore, DtnPeer};
pub use crate::routing::RoutingNotifcation;

use anyhow::Result;
use lazy_static::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::IpAddr;
use tokio::sync::mpsc::{channel, Receiver, Sender};

lazy_static! {
    pub static ref CONFIG: Mutex<DtnConfig> = Mutex::new(DtnConfig::new());
    pub static ref DTNCORE: Mutex<DtnCore> = Mutex::new(DtnCore::new());
    pub static ref PEERS: Mutex<HashMap<String, DtnPeer>> = Mutex::new(HashMap::new());
    pub static ref STATS: Mutex<DtnStatistics> = Mutex::new(DtnStatistics::new());
    pub static ref SENDERTASK: Mutex<Option<Sender<Bundle>>> = Mutex::new(None);
    pub static ref STORE: Mutex<Box<dyn BundleStore + Send>> =
        Mutex::new(Box::new(InMemoryBundleStore::new()));
}

pub fn cla_add(cla: Box<dyn ConvergenceLayerAgent>) {
    (*DTNCORE.lock()).cl_list.push(cla);
}
pub fn service_add(tag: u8, service: String) {
    (*DTNCORE.lock()).service_list.insert(tag, service);
}
pub fn add_discovery_destination(destination: &String) {
    (*CONFIG.lock())
        .discovery_destinations
        .insert(destination.clone(), 0);
}

pub fn reset_sequence(destination: &String) {
    if let Some(sequence) = (*CONFIG.lock()).discovery_destinations.get_mut(destination) {
        *sequence = 0;
    }
}
pub fn get_sequence(destination: &String) -> u32 {
    if let Some(sequence) = (*CONFIG.lock()).discovery_destinations.get(destination) {
        *sequence
    } else {
        0
    }
}
pub fn peers_add(peer: DtnPeer) {
    (*PEERS.lock()).insert(peer.eid.node().unwrap(), peer);
}
pub fn peers_count() -> usize {
    (*PEERS.lock()).len()
}
pub fn peers_clear() {
    (*PEERS.lock()).clear();
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
    eid.node_id() == (*CONFIG.lock()).host_eid.node_id()
}
pub fn peers_cla_for_node(eid: &EndpointID) -> Option<crate::cla::ClaSender> {
    if let Some(peer) = peers_get_for_node(eid) {
        return peer.first_cla();
    }
    None
}
pub fn peer_find_by_remote(addr: &IpAddr) -> Option<String> {
    for (_, p) in (*PEERS.lock()).iter() {
        if p.addr() == addr {
            return Some(p.node_name());
        }
    }
    None
}

pub fn store_push(bp: &BundlePack) -> Result<()> {
    (*STORE.lock()).push(&bp)
}

pub fn store_remove(bid: &str) {
    (*STORE.lock()).remove(bid);
}

pub fn store_update(bp: &BundlePack) -> Result<()> {
    (*STORE.lock()).update(bp)
}
pub fn store_has_item(bid: &str) -> bool {
    (*STORE.lock()).has_item(bid)
}
pub fn store_get(bpid: &str) -> Option<BundlePack> {
    Some((*STORE.lock()).get(bpid)?.clone())
}

pub fn store_delete_expired() {
    let pending_bids: Vec<String> = (*STORE.lock()).pending();

    let expired: Vec<String> = pending_bids
        .iter()
        .map(|b| (*STORE.lock()).get(b))
        //.filter(|b| b.is_some())
        //.map(|b| b.unwrap())
        .filter_map(|b| b)
        .filter(|e| e.bundle.primary.is_lifetime_exceeded())
        .map(|e| e.id().into())
        .collect();
    for bid in expired {
        store_remove(&bid);
    }
}
pub fn routing_notify(notification: RoutingNotifcation) {
    (*DTNCORE.lock()).routing_agent.notify(notification);
}
