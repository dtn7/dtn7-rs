use super::RoutingAgent;
use crate::cla::{ClaSenderTask, ConvergenceLayerAgent};
use crate::core::bundlepack::BundlePack;
use crate::{CLAS, PEERS};

/// Simple flooding-basic routing.
/// All bundles are sent to all known peers again and again.
#[derive(Default, Debug)]
pub struct FloodingRoutingAgent {}

impl FloodingRoutingAgent {
    pub fn new() -> FloodingRoutingAgent {
        FloodingRoutingAgent {}
    }
}
impl std::fmt::Display for FloodingRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FloodingRoutingAgent")
    }
}

impl RoutingAgent for FloodingRoutingAgent {
    fn sender_for_bundle(&mut self, _bp: &BundlePack) -> (Vec<ClaSenderTask>, bool) {
        let mut clas = Vec::new();
        for (_, p) in (*PEERS.lock()).iter() {
            for p2 in &p.cla_list {
                for c in (*CLAS.lock()).iter() {
                    if c.name() == p2.0 {
                        let dest = if let Some(port) = p2.1 {
                            format!("{}:{}", p.addr(), port)
                        } else {
                            p.addr().to_string()
                        };
                        let cla = ClaSenderTask {
                            cla: c.clone(),
                            dest,
                            next_hop: p.eid.clone(),
                        };
                        clas.push(cla);
                    }
                }
            }
        }
        (clas, false)
    }
}
