pub mod cla;
pub mod core;
pub mod dtnconfig;
pub mod dtnd;
pub mod routing;

use crate::cla::ConvergencyLayerAgent;
use crate::core::bundlepack::BundlePack;
use crate::core::store::{BundleStore, SimpleBundleStore};
use crate::core::DtnStatistics;
use bp7::EndpointID;
pub use dtnconfig::DtnConfig;

pub use crate::core::{DtnCore, DtnPeer};
pub use crate::routing::RoutingNotifcation;

use lazy_static::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::IpAddr;

lazy_static! {
    pub static ref CONFIG: Mutex<DtnConfig> = Mutex::new(DtnConfig::new());
    pub static ref DTNCORE: Mutex<DtnCore> = Mutex::new(DtnCore::new());
    pub static ref PEERS: Mutex<HashMap<String, DtnPeer>> = Mutex::new(HashMap::new());
    pub static ref STATS: Mutex<DtnStatistics> = Mutex::new(DtnStatistics::new());
    pub static ref STORE: Mutex<Box<dyn BundleStore + Send>> =
        Mutex::new(Box::new(SimpleBundleStore::new()));
}

pub fn cla_add(cla: Box<dyn ConvergencyLayerAgent>) {
    (*DTNCORE.lock()).cl_list.push(cla);
}
pub fn peers_add(peer: DtnPeer) {
    (*PEERS.lock()).insert(peer.eid.node_part().unwrap(), peer);
}
pub fn peers_count() -> usize {
    (*PEERS.lock()).len()
}
pub fn peers_clear() {
    (*PEERS.lock()).clear();
}
pub fn peers_get_for_node(eid: &EndpointID) -> Option<DtnPeer> {
    for (_, p) in (*PEERS.lock()).iter() {
        if p.node_name() == eid.node_part().unwrap_or_default() {
            return Some(p.clone());
        }
    }
    None
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

pub fn store_push(bp: &BundlePack) {
    (*STORE.lock()).push(&bp);
}

pub fn store_remove(bid: &str) {
    (*STORE.lock()).remove(bid);
}

pub fn store_update(bp: &BundlePack) {
    (*STORE.lock()).update(bp);
}
pub fn store_has_item(bp: &BundlePack) -> bool {
    (*STORE.lock()).has_item(&bp)
}
pub fn store_get(bpid: &str) -> Option<BundlePack> {
    Some((*STORE.lock()).get(bpid)?.clone())
}

pub fn routing_notify(notification: RoutingNotifcation) {
    (*DTNCORE.lock()).routing_agent.notify(notification);
}
