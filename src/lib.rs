pub mod core;

pub mod dtnd;

pub mod cla;

pub mod dtnconfig;

pub mod routing;

use crate::cla::ConvergencyLayerAgent;
use crate::core::bundlepack::BundlePack;
use crate::core::store::{BundleStore, SimpleBundleStore};
use crate::core::DtnStatistics;
use bp7::EndpointID;
pub use dtnconfig::DtnConfig;

pub use crate::core::{DtnCore, DtnPeer};

use lazy_static::*;
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    pub static ref CONFIG: Mutex<DtnConfig> = Mutex::new(DtnConfig::new());
    pub static ref DTNCORE: Mutex<DtnCore> = Mutex::new(DtnCore::new());
    pub static ref PEERS: Mutex<HashMap<String, DtnPeer>> = Mutex::new(HashMap::new());
    pub static ref STATS: Mutex<DtnStatistics> = Mutex::new(DtnStatistics::new());
    pub static ref STORE: Mutex<Box<dyn BundleStore + Send>> =
        Mutex::new(Box::new(SimpleBundleStore::new()));
}

pub fn cla_add(cla: Box<dyn ConvergencyLayerAgent>) {
    DTNCORE.lock().unwrap().cl_list.push(cla);
}
pub fn peers_add(peer: DtnPeer) {
    PEERS
        .lock()
        .unwrap()
        .insert(peer.eid.node_part().unwrap(), peer);
}
pub fn peers_count() -> usize {
    PEERS.lock().unwrap().len()
}
pub fn peers_clear() {
    PEERS.lock().unwrap().clear();
}
pub fn peers_get_for_node(eid: &EndpointID) -> Option<DtnPeer> {
    for (_, p) in PEERS.lock().unwrap().iter() {
        if p.get_node_name() == eid.node_part().unwrap_or_default() {
            return Some(p.clone());
        }
    }
    None
}
pub fn peers_cla_for_node(eid: &EndpointID) -> Option<crate::cla::ClaSender> {
    if let Some(peer) = peers_get_for_node(eid) {
        return peer.get_first_cla();
    }
    None
}

pub fn store_push(bp: &BundlePack) {
    STORE.lock().unwrap().push(&bp);
}

pub fn store_has_item(bp: &BundlePack) -> bool {
    STORE.lock().unwrap().has_item(&bp)
}
