pub mod application_agent;
pub mod bundlepack;
pub mod helpers;
pub mod peer;
pub mod processing;
pub mod store;

use crate::cla::CLAEnum;
pub use crate::core::peer::{DtnPeer, PeerType};
use crate::core::store::BundleStore;
use crate::routing::RoutingAgent;
use crate::routing::RoutingAgentsEnum;
use crate::{store_get_bundle, store_get_metadata};
pub use crate::{store_has_item, store_push_bundle};
use crate::{PEERS, STORE};
use application_agent::ApplicationAgent;
use bp7::EndpointID;
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::core::application_agent::ApplicationAgentEnum;

use self::processing::forward;

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
    pub endpoints: Vec<ApplicationAgentEnum>,
    pub cl_list: Vec<CLAEnum>,
    pub service_list: HashMap<u8, String>,
    pub routing_agent: RoutingAgentsEnum,
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
            //routing_agent: crate::routing::flooding::FloodingRoutingAgent::new().into(),
            routing_agent: crate::routing::epidemic::EpidemicRoutingAgent::new().into(),
        }
    }

    pub fn register_application_agent(&mut self, aa: ApplicationAgentEnum) {
        if self.is_in_endpoints(aa.eid()) {
            info!("Application agent already registered for EID: {}", aa.eid());
        } else {
            info!("Registered new application agent for EID: {}", aa.eid());
            self.endpoints.push(aa);
        }
    }
    pub fn unregister_application_agent(&mut self, aa: ApplicationAgentEnum) {
        info!("Unregistered application agent for EID: {}", aa.eid());
        self.endpoints
            .iter()
            .position(|n| n.eid() == aa.eid())
            .map(|e| self.endpoints.remove(e));
    }
    pub fn eids(&self) -> Vec<String> {
        self.endpoints.iter().map(|e| e.eid().to_string()).collect()
    }
    pub fn bundle_ids(&self) -> Vec<String> {
        (*STORE.lock()).all_ids()
    }
    pub fn bundle_count(&self) -> usize {
        (*STORE.lock()).count() as usize
    }
    pub fn bundle_names(&self) -> Vec<String> {
        (*STORE.lock()).all_ids()
    }
    pub fn is_in_endpoints(&self, eid: &EndpointID) -> bool {
        for aa in self.endpoints.iter() {
            if eid == aa.eid() {
                return true;
            }
        }
        false
    }
    pub fn get_endpoint_mut(&mut self, eid: &EndpointID) -> Option<&mut ApplicationAgentEnum> {
        for aa in self.endpoints.iter_mut() {
            if eid == aa.eid() {
                return Some(aa);
            }
        }
        None
    }
    pub fn get_endpoint(&self, eid: &EndpointID) -> Option<&ApplicationAgentEnum> {
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
    (*PEERS.lock()).retain(|_k, v| {
        let val = v.still_valid();
        if !val {
            info!(
                "Have not seen {} @ {} in a while, removing it from list of known peers",
                v.eid, v.addr
            );
        }
        v.con_type == PeerType::Static || val
    });
}

/// Reprocess bundles in store
pub async fn process_bundles() {
    let forwarding_bundle_ids: Vec<String> = (*STORE.lock()).forwarding();
    for bpid in forwarding_bundle_ids {
        if let Err(err) = forward(store_get_metadata(&bpid).unwrap()).await {
            error!("Error forwarding bundle: {}", err);
        }
    }
    //forwarding_bundle_ids.iter().for_each(|bpid| {});
}
