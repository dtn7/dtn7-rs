use super::RoutingAgent;
use crate::cla::ClaSender;
use crate::core::bundlepack::BundlePack;
use crate::routing::RoutingNotifcation;
use crate::PEERS;
use log::debug;
use std::collections::{HashMap, HashSet};

/// Simple epidemic routing.
/// All bundles are sent to all known peers once via all CLAs.
#[derive(Default, Debug)]
pub struct EpidemicRoutingAgent {
    history: HashMap<String, HashSet<String>>,
}

impl EpidemicRoutingAgent {
    pub fn new() -> EpidemicRoutingAgent {
        EpidemicRoutingAgent {
            history: HashMap::new(),
        }
    }
    fn add(&mut self, bundle_id: String, node_name: String) {
        let entries = self.history.entry(bundle_id).or_insert_with(HashSet::new);
        entries.insert(node_name);
    }
    /*fn remove_bundle(&mut self, bundle_id: String) {
        self.history.remove(&bundle_id);
    }*/
    /*fn filtered(&mut self, dest: String, bundles: &[ByteBuffer]) -> Vec<ByteBuffer> {
        let entries = self.history.entry(dest).or_insert_with(HashSet::new);
        bundles
            .iter()
            .cloned()
            .filter(|b| !entries.contains(b))
            .collect()
    }*/
    fn contains(&mut self, bundle_id: &str, node_name: &str) -> bool {
        if let Some(entries) = self.history.get(bundle_id) {
            //let entries = self.history.entry(bundle_id);
            return entries.contains(node_name);
        }
        false
    }
    fn sending_failed(&mut self, bundle_id: &str, node_name: &str) {
        if let Some(entries) = self.history.get_mut(bundle_id) {
            entries.remove(node_name);
            debug!(
                "removed {:?} from sent list for bundle {}",
                node_name, bundle_id
            );
        }
    }
    fn incoming_bundle(&mut self, bundle_id: &str, node_name: &str) {
        if !node_name.is_empty() && !self.contains(bundle_id, node_name) {
            self.add(bundle_id.to_string(), node_name.to_string());
        }
    }
}
impl std::fmt::Display for EpidemicRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "EpidemicRoutingAgent")
    }
}
impl RoutingAgent for EpidemicRoutingAgent {
    fn notify(&mut self, notification: RoutingNotifcation) {
        match notification {
            RoutingNotifcation::SendingFailed(bid, cla_sender) => {
                self.sending_failed(bid, cla_sender);
            }
            RoutingNotifcation::IncomingBundle(bndl) => {
                if let Some(eid) = bndl.previous_node() {
                    if let Some(node_name) = eid.node() {
                        self.incoming_bundle(&bndl.id(), &node_name);
                    }
                };
            }
            RoutingNotifcation::IncomingBundleWithoutPreviousNode(bid, node_name) => {
                self.incoming_bundle(bid, node_name);
            }
            _ => {}
        }
    }
    fn sender_for_bundle(&mut self, bp: &BundlePack) -> (Vec<ClaSender>, bool) {
        let mut clas = Vec::new();
        for (_, p) in (*PEERS.lock()).iter() {
            if let Some(cla) = p.first_cla() {
                if !self.contains(bp.id(), &p.node_name()) {
                    clas.push(cla);
                    self.add(bp.id().to_string(), p.node_name().clone());
                }
            }
        }
        (clas, false)
    }
}
