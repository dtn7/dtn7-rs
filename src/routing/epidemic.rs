use super::RoutingAgent;
use crate::cla::ConvergencyLayerAgent;
use bp7::ByteBuffer;
use std::collections::{HashMap, HashSet};

/// Simple epidemic routing.
/// All bundles are sent to all known peers once via all CLAs.
#[derive(Default, Debug)]
pub struct EpidemicRoutingAgent {
    history: HashMap<String, HashSet<ByteBuffer>>,
}

impl EpidemicRoutingAgent {
    pub fn new() -> EpidemicRoutingAgent {
        EpidemicRoutingAgent {
            history: HashMap::new(),
        }
    }
    fn add(&mut self, dest: String, bundles: &[ByteBuffer]) {
        let entries = self
            .history
            .entry(dest.clone())
            .or_insert_with(HashSet::new);
        for b in bundles {
            entries.insert(b.to_vec());
        }
    }
    fn remove(&mut self, dest: String) {
        self.history.remove(&dest);
    }
    fn filtered(&mut self, dest: String, bundles: &[ByteBuffer]) -> Vec<ByteBuffer> {
        let entries = self.history.entry(dest).or_insert_with(HashSet::new);
        bundles
            .iter()
            .cloned()
            .filter(|b| !entries.contains(b))
            .collect()
    }
}
impl std::fmt::Display for EpidemicRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "EpidemicRoutingAgent")
    }
}
impl RoutingAgent for EpidemicRoutingAgent {
    fn route_bundle(
        &mut self,
        _bundle: &ByteBuffer,
        _peers: Vec<String>,
        _cl_list: &[Box<dyn ConvergencyLayerAgent>],
    ) {
        unimplemented!();
    }
    fn route_all(
        &mut self,
        bundles: Vec<ByteBuffer>,
        peers: Vec<String>,
        cl_list: &[Box<dyn ConvergencyLayerAgent>],
    ) {
        for p in &peers {
            // Send each bundle to any known peer once per CLA.. ignoring whether transmission was successful or not
            let b_list = self.filtered(p.to_string(), &bundles);
            for cla in &mut cl_list.iter() {
                if !b_list.is_empty() {
                    cla.scheduled_submission(&b_list, &p);
                }
            }
            if !b_list.is_empty() {
                self.add(p.to_string(), &b_list);
            }
        }
    }
}
