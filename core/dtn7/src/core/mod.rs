pub mod application_agent;
pub mod bundlepack;
pub mod helpers;
pub mod peer;
pub mod processing;
pub mod stats;
pub mod store;

use crate::cla::ConvergenceLayerAgent;
use crate::core::bundlepack::Constraint;
pub use crate::core::peer::{DtnPeer, PeerType};
use crate::core::stats::{NodeStats, RegistrationInformation};
use crate::core::store::BundleStore;
use crate::routing::RoutingAgentsEnum;
use crate::{
    routing_notify, store_delete_expired, store_get_bundle, store_get_metadata, CLAS, DTNCORE,
};
pub use crate::{store_has_item, store_push_bundle};
use crate::{RoutingNotifcation, CONFIG};
use crate::{PEERS, STORE};
use application_agent::ApplicationAgent;
use bp7::EndpointID;
use log::{error, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use crate::core::application_agent::ApplicationAgentEnum;

use self::bundlepack::BundlePack;
use self::processing::forward;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct DtnStatistics {
    pub incoming: u64,
    pub dups: u64,
    pub outgoing: u64,
    pub delivered: u64,
    pub failed: u64,
    pub broken: u64,
    pub node: NodeStats,
}

impl DtnStatistics {
    pub fn new() -> DtnStatistics {
        let nodestats = NodeStats::new();
        DtnStatistics {
            incoming: 0,
            dups: 0,
            outgoing: 0,
            delivered: 0,
            failed: 0,
            broken: 0,
            node: nodestats,
        }
    }
    pub fn update_node_stats(&mut self) {
        println!("Updating node stats");
        self.node.error_info.failed_forwards_bundle_count = self.failed;
        self.node.registrations.clear();
        let eids = (*DTNCORE.lock()).eids();
        for eid in eids {
            if let Some(aa) =
                (*DTNCORE.lock()).get_endpoint(&EndpointID::try_from(eid.clone()).unwrap())
            {
                let singleton = !aa.eid().is_non_singleton();
                let registration = RegistrationInformation {
                    eid: eid.clone(),
                    active: aa.delivery_addr().is_some(),
                    singleton,
                    default_failure_action: stats::FailureAction::Defer,
                };
                self.node.registrations.push(registration);
            }
        }
        self.node.bundles.bundles_stored = (*STORE.lock()).count();
        self.node.bundles.forward_pending_bundle_count = (*STORE.lock()).forwarding().len() as u64;
        // TODO get correct number of bundles with dispatch pending
        // self.node.bundles.dispatch_pending_bundle_count = (*STORE.lock()).pending().len() as u64;
        // TODO get correct number of bundles with reassembly pending
        // self.node.bundles.reassembly_pending_bundle_count =
        //     (*STORE.lock()).reassembly_pending().len() as u64;
    }
}
#[derive(Debug)]
pub struct DtnCore {
    pub endpoints: Vec<ApplicationAgentEnum>,
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
    pub fn bundle_full_meta(&self) -> Vec<String> {
        (*STORE.lock()).src_dst_ts()
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
        self.endpoints.iter_mut().find(|aa| eid == aa.eid())
    }
    pub fn get_endpoint(&self, eid: &EndpointID) -> Option<&ApplicationAgentEnum> {
        self.endpoints.iter().find(|&aa| eid == aa.eid())
    }
}

/// Removes peers from global peer list that haven't been seen in a while.
pub async fn process_peers() {
    let mut dropped: Vec<EndpointID> = Vec::new();

    (*PEERS.lock()).retain(|_k, v| {
        let val = v.still_valid();
        if !val {
            info!(
                "Have not seen {} @ {} in a while, removing it from list of known peers",
                v.eid, v.addr
            );

            dropped.push(v.eid.clone());
        }
        v.con_type == PeerType::Static || val
    });

    for eid in dropped {
        if let Err(err) = routing_notify(RoutingNotifcation::DroppedPeer(eid)).await {
            error!("Error while dropping peer: {}", err);
        }
    }
}

/// Reprocess bundles in store
pub async fn process_bundles() {
    let now_total = Instant::now();

    store_delete_expired();

    let active_cla = (*CLAS.lock()).iter().any(|p| p.accepting());
    if !active_cla {
        warn!("No active/push CLA, not forwarding any bundles");
        trace!("time to process bundles: {:?}", now_total.elapsed());
        return;
    }

    let forwarding_bids: Vec<String> = (*STORE.lock()).forwarding();

    let mut forwarding_bundles: Vec<BundlePack> = forwarding_bids
        .iter()
        .filter_map(|bid| store_get_metadata(bid))
        .filter(|bp| !bp.has_constraint(Constraint::Deleted))
        .collect();

    // process them in chronological order
    forwarding_bundles.sort_unstable_by(|a, b| a.creation_time.cmp(&b.creation_time));

    let num_bundles = forwarding_bundles.len();

    if CONFIG.lock().parallel_bundle_processing {
        let mut tasks = Vec::new();
        for bp in forwarding_bundles {
            let bpid = bp.id().to_string();
            let task_handle = tokio::spawn(async move {
                let now = Instant::now();
                if let Err(err) = forward(bp).await {
                    error!("Error forwarding bundle: {}", err);
                }
                trace!("Forwarding time: {:?} for {}", now.elapsed(), bpid);
            });
            tasks.push(task_handle);
        }
        use futures::future::join_all;

        join_all(tasks).await;
    } else {
        for bp in forwarding_bundles {
            let bpid = bp.id().to_string();

            let now = Instant::now();
            if let Err(err) = forward(bp).await {
                error!("Error forwarding bundle: {}", err);
            }
            trace!("Forwarding time: {:?} for {}", now.elapsed(), bpid);
        }
    }

    trace!(
        "time to process {} bundles: {:?}",
        num_bundles,
        now_total.elapsed()
    );
    //forwarding_bundle_ids.iter().for_each(|bpid| {});
}
