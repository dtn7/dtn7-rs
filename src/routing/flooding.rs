use super::RoutingAgent;
use crate::core::DtnCore;
use bp7::Bundle;

#[derive(Debug)]
pub struct FloodingRoutingAgent {}

impl FloodingRoutingAgent {
    pub fn new() -> FloodingRoutingAgent {
        FloodingRoutingAgent {}
    }
}
impl RoutingAgent for FloodingRoutingAgent {
    fn route_bundle(&mut self, bundle: &Bundle) {
        unimplemented!();
    }
    fn route_all(&mut self, core: &mut DtnCore) {
        unimplemented!();
    }
}
