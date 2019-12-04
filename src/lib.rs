#![feature(proc_macro_hygiene, decl_macro)]
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

use lazy_static::*;
use parking_lot::Mutex;
use std::collections::HashMap;

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
    println!("peers_add");
    (*PEERS.lock()).insert(dbg!(peer.eid.node_part().unwrap()), peer);
    dbg!(&(*PEERS.lock()));
    dbg!((*PEERS.lock()).len());
    println!("peers_add done");
}
pub fn peers_count() -> usize {
    dbg!((*PEERS.lock()).len())
}
pub fn peers_clear() {
    println!("peers_clear");
    dbg!(&(*PEERS.lock()));
    (*PEERS.lock()).clear();
    dbg!(&(*PEERS.lock()));
    println!("peers_clear done");
}
pub fn peers_get_for_node(eid: &EndpointID) -> Option<DtnPeer> {
    for (_, p) in (*PEERS.lock()).iter() {
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
    (*STORE.lock()).push(&bp);
}

pub fn store_has_item(bp: &BundlePack) -> bool {
    (*STORE.lock()).has_item(&bp)
}
pub fn store_get(bpid: &str) -> Option<BundlePack> {
    Some((*STORE.lock()).get(bpid)?.clone())
}
