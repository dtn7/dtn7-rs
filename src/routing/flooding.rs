use super::RoutingAgent;
use crate::cla::CLA_sender;
use crate::cla::ConvergencyLayerAgent;
use crate::core::bundlepack::BundlePack;
use crate::PEERS;
use bp7::ByteBuffer;

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
    fn sender_for_bundle(&mut self, bp: &BundlePack) -> (Vec<CLA_sender>, bool) {
        let mut clas = Vec::new();
        for (_, p) in PEERS.lock().unwrap().iter() {
            if let Some(cla) = p.get_first_cla() {
                clas.push(cla);
            }
        }
        (clas, false)
    }
}
