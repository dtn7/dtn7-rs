pub mod epidemic;
pub mod flooding;

use crate::cla::{CLA_sender, ConvergencyLayerAgent};
use crate::core::bundlepack::BundlePack;
use bp7::ByteBuffer;
use std::fmt::Debug;
use std::fmt::Display;

pub trait RoutingAgent: Debug + Send + Display {
    fn sender_for_bundle(&mut self, bp: &BundlePack) -> (Vec<CLA_sender>, bool) {
        unimplemented!();
    }
}
pub fn routing_algorithms() -> Vec<&'static str> {
    vec!["flooding", "epidemic"]
}

pub fn new(routingagent: &str) -> Box<RoutingAgent> {
    match routingagent {
        "flooding" => Box::new(flooding::FloodingRoutingAgent::new()),
        "epidemic" => Box::new(epidemic::EpidemicRoutingAgent::new()),
        _ => panic!("Unknown routing agent {}", routingagent),
    }
}
