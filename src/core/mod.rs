pub mod application_agent;
pub mod bundlepack;
pub mod helpers;
pub mod peer;
pub mod processing;
pub mod store;

use crate::cla::ConvergenceLayerAgent;
pub use crate::core::peer::{DtnPeer, PeerType};
use crate::core::store::BundleStore;
use crate::routing::RoutingAgent;
use crate::store_get;
pub use crate::{store_has_item, store_push};
use crate::{PEERS, STORE};
use anyhow::Result;
use application_agent::ApplicationAgent;
use bp7::EndpointID;
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct DtnStatistics {
    pub incoming: u64,
    pub dups: u64,
    pub outgoing: u64,
    pub delivered: u64,
    pub broken: u64,
}

impl DtnStatistics {
    pub fn new() -> DtnStatistics {
        DtnStatistics {
            incoming: 0,
            dups: 0,
            outgoing: 0,
            delivered: 0,
            broken: 0,
        }
    }
}
#[derive(Debug)]
pub struct DtnCore {
    pub endpoints: Vec<Box<dyn ApplicationAgent + Send>>,
    pub cl_list: Vec<Box<dyn ConvergenceLayerAgent>>,
    pub service_list: HashMap<u8, String>,
    pub routing_agent: Box<dyn RoutingAgent>,
}

impl Default for DtnCore {
    fn default() -> Self {
        Self::new()
    }
}

impl DtnCore {
    pub fn new() -> DtnCore {
        DtnCore {
            endpoints: Vec::new(),
            cl_list: Vec::new(),
            service_list: HashMap::new(),
            //routing_agent: Box::new(crate::routing::flooding::FloodingRoutingAgent::new()),
            routing_agent: Box::new(crate::routing::epidemic::EpidemicRoutingAgent::new()),
        }
    }

    pub fn register_application_agent<T: 'static + ApplicationAgent + Send>(&mut self, aa: T) {
        if self.is_in_endpoints(&aa.eid()) {
            info!("Application agent already registered for EID: {}", aa.eid());
        } else {
            info!("Registered new application agent for EID: {}", aa.eid());
            self.endpoints.push(Box::new(aa));
        }
    }
    pub fn unregister_application_agent<T: 'static + ApplicationAgent>(&mut self, aa: T) {
        info!("Unregistered application agent for EID: {}", aa.eid());
        self.endpoints
            .iter()
            .position(|n| n.eid() == aa.eid())
            .map(|e| self.endpoints.remove(e));
    }
    pub fn eids(&self) -> Vec<String> {
        self.endpoints.iter().map(|e| e.eid().to_string()).collect()
    }
    pub fn bundles(&self) -> Vec<String> {
        (*STORE.lock())
            .bundles()
            .iter()
            .map(|e| e.id().to_string())
            .collect()
    }
    pub fn bundle_names(&self) -> Vec<String> {
        (*STORE.lock())
            .bundles()
            .iter()
            .map(|e| e.bundle.to_string())
            .collect()
    }
    pub fn is_in_endpoints(&self, eid: &EndpointID) -> bool {
        for aa in self.endpoints.iter() {
            if eid == aa.eid() {
                return true;
            }
        }
        false
    }
    pub fn get_endpoint_mut(
        &mut self,
        eid: &EndpointID,
    ) -> Option<&mut Box<dyn ApplicationAgent + Send>> {
        for aa in self.endpoints.iter_mut() {
            if eid == aa.eid() {
                return Some(aa);
            }
        }
        None
    }
    pub fn get_endpoint(&self, eid: &EndpointID) -> Option<&Box<dyn ApplicationAgent + Send>> {
        for aa in self.endpoints.iter() {
            if eid == aa.eid() {
                return Some(aa);
            }
        }
        None
    }
}

/// Removes peers from global peer list that haven't been seen in a while.
pub fn process_peers() {
    (*PEERS.lock()).retain(|_k, v| v.con_type == PeerType::Static || v.still_valid());
}

/// Reprocess bundles in store
pub async fn process_bundles() {
    // TODO: check for possible race condition and double send when janitor is triggered while first forwarding attempt is in progress
    let forwarding_bundle_ids: Vec<String> = (*STORE.lock()).forwarding();
    for bpid in forwarding_bundle_ids {
        crate::core::processing::forward(store_get(&bpid).unwrap()).await;
    }
    //forwarding_bundle_ids.iter().for_each(|bpid| {});
}
