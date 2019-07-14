use super::RoutingAgent;
use crate::cla::CLA_sender;
use crate::cla::ConvergencyLayerAgent;
use crate::core::bundlepack::BundlePack;
use crate::PEERS;
use bp7::ByteBuffer;
use std::collections::{HashMap, HashSet};

/// Simple epidemic routing.
/// All bundles are sent to all known peers once via all CLAs.
#[derive(Default, Debug)]
pub struct EpidemicRoutingAgent {
    history: HashMap<String, HashSet<CLA_sender>>,
}

impl EpidemicRoutingAgent {
    pub fn new() -> EpidemicRoutingAgent {
        EpidemicRoutingAgent {
            history: HashMap::new(),
        }
    }
    fn add(&mut self, bundle_id: String, cla_sender: CLA_sender) {
        let entries = self.history.entry(bundle_id).or_insert_with(HashSet::new);
        entries.insert(cla_sender);
    }
    fn remove_bundle(&mut self, bundle_id: String) {
        self.history.remove(&bundle_id);
    }
    /*fn filtered(&mut self, dest: String, bundles: &[ByteBuffer]) -> Vec<ByteBuffer> {
        let entries = self.history.entry(dest).or_insert_with(HashSet::new);
        bundles
            .iter()
            .cloned()
            .filter(|b| !entries.contains(b))
            .collect()
    }*/
    fn contains(&mut self, bundle_id: &String, cla_sender: &CLA_sender) -> bool {
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
    fn sender_for_bundle(&mut self, bp: &BundlePack) -> (Vec<CLA_sender>, bool) {
        let mut clas = Vec::new();
        for (_, p) in PEERS.lock().unwrap().iter() {
            if let Some(cla) = p.get_first_cla() {
                if !self.contains(&bp.id(), &cla) {
                    clas.push(cla.clone());
                    self.add(bp.id(), cla);
                }
            }
        }
        (clas, false)
    }
}
