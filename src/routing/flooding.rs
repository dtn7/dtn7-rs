use super::RoutingAgent;
use crate::core::core::DtnCore;
use bp7::Bundle;

pub struct FloodingRoutingAgent {}

impl RoutingAgent for FloodingRoutingAgent {
    fn route_bundle(&mut self, bundle: &Bundle) {
        unimplemented!();
    }
    fn route_all(&mut self, core: &mut DtnCore) {
        unimplemented!();
    }
}
