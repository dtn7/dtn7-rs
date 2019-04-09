pub mod flooding;

use crate::core::core::DtnCore;
use bp7::Bundle;

pub trait RoutingAgent {
    fn route_bundle(&mut self, bundle: &Bundle) {
        unimplemented!();
    }
    fn route_all(&mut self, core: &mut DtnCore) {
        unimplemented!();
    }
}
