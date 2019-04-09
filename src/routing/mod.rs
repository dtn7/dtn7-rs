pub mod flooding;

use crate::core::DtnCore;
use bp7::Bundle;
use std::fmt::Debug;

pub trait RoutingAgent: Debug + Send {
    fn route_bundle(&mut self, bundle: &Bundle) {
        unimplemented!();
    }
    fn route_all(&mut self, core: &mut DtnCore) {
        unimplemented!();
    }
}
