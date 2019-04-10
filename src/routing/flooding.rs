use super::RoutingAgent;
use crate::cla::ConvergencyLayerAgent;
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
    fn route_bundle(
        &mut self,
        bundle: &ByteBuffer,
        peers: Vec<String>,
        cl_list: &[Box<dyn ConvergencyLayerAgent>],
    ) {
        unimplemented!();
    }
    fn route_all(
        &mut self,
        bundles: Vec<ByteBuffer>,
        peers: Vec<String>,
        cl_list: &[Box<dyn ConvergencyLayerAgent>],
    ) {
        for cla in &mut cl_list.iter() {
            cla.scheduled_process(&bundles, &peers);
        }
    }
}
