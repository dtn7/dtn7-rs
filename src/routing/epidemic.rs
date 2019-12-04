use super::RoutingAgent;
use crate::cla::ClaSender;
use crate::core::bundlepack::BundlePack;
use crate::PEERS;
use std::collections::{HashMap, HashSet};

/// Simple epidemic routing.
/// All bundles are sent to all known peers once via all CLAs.
#[derive(Default, Debug)]
pub struct EpidemicRoutingAgent {
    history: HashMap<String, HashSet<ClaSender>>,
}

impl EpidemicRoutingAgent {
    pub fn new() -> EpidemicRoutingAgent {
        EpidemicRoutingAgent {
            history: HashMap::new(),
        }
    }
    fn add(&mut self, bundle_id: String, cla_sender: ClaSender) {
        let entries = self.history.entry(bundle_id).or_insert_with(HashSet::new);
        entries.insert(cla_sender);
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
    fn contains(&mut self, bundle_id: &str, cla_sender: &ClaSender) -> bool {
        if let Some(entries) = self.history.get(bundle_id) {
            //let entries = self.history.entry(bundle_id);
            return entries.contains(cla_sender);
        }
        false
    }
}
impl std::fmt::Display for EpidemicRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "EpidemicRoutingAgent")
    }
}
impl RoutingAgent for EpidemicRoutingAgent {
    fn sender_for_bundle(&mut self, bp: &BundlePack) -> (Vec<ClaSender>, bool) {
        let mut clas = Vec::new();
        for (_, p) in (*PEERS.lock()).iter() {
            if let Some(cla) = p.get_first_cla() {
                if !self.contains(&bp.id(), &cla) {
                    clas.push(cla.clone());
                    self.add(bp.id().to_string(), cla);
                }
            }
        }
        (clas, false)
    }
}
