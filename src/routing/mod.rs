pub mod epidemic;
pub mod flooding;
pub mod sink;

use crate::cla::ClaSender;
use crate::core::bundlepack::BundlePack;
use std::fmt::Debug;
use std::fmt::Display;

pub enum RoutingNotifcation<'a> {
    SendingFailed(&'a str, &'a str),
    IncomingBundle(&'a str, &'a str),
}

pub trait RoutingAgent: Debug + Send + Display {
    fn notify(&mut self, _notification: RoutingNotifcation) {}
    fn sender_for_bundle(&mut self, _bp: &BundlePack) -> (Vec<ClaSender>, bool) {
        unimplemented!();
    }
}

pub fn routing_algorithms() -> Vec<&'static str> {
    vec!["epidemic", "flooding", "sink"]
}

pub fn new(routingagent: &str) -> Box<dyn RoutingAgent> {
    match routingagent {
        "flooding" => Box::new(flooding::FloodingRoutingAgent::new()),
        "epidemic" => Box::new(epidemic::EpidemicRoutingAgent::new()),
        "sink" => Box::new(sink::SinkRoutingAgent::new()),
        _ => panic!("Unknown routing agent {}", routingagent),
    }
}
