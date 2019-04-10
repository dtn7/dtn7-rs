pub mod epidemic;
pub mod flooding;

use crate::cla::ConvergencyLayerAgent;
use bp7::ByteBuffer;
use std::fmt::Debug;
use std::fmt::Display;

pub trait RoutingAgent: Debug + Send + Display {
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
        _bundles: Vec<ByteBuffer>,
        _peers: Vec<String>,
        _cl_list: &[Box<dyn ConvergencyLayerAgent>],
    ) {
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
