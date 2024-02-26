pub mod epidemic;
pub mod erouting;
pub mod external;
pub mod flooding;
pub mod sink;
pub mod sprayandwait;
pub mod static_routing;

use crate::cla::ClaSenderTask;
use crate::core::bundlepack::BundlePack;
use async_trait::async_trait;
use bp7::Bundle;
use bp7::EndpointID;
use derive_more::*;
use enum_dispatch::enum_dispatch;
use epidemic::EpidemicRoutingAgent;
use external::ExternalRoutingAgent;
use flooding::FloodingRoutingAgent;
use sink::SinkRoutingAgent;
use sprayandwait::SprayAndWaitRoutingAgent;
use static_routing::StaticRoutingAgent;
use std::fmt::Debug;
use std::fmt::Display;
use tokio::sync::{mpsc, oneshot};

pub enum RoutingNotifcation {
    SendingFailed(String, String),
    IncomingBundle(Bundle),
    IncomingBundleWithoutPreviousNode(String, String),
    EncounteredPeer(EndpointID),
    DroppedPeer(EndpointID),
}

#[enum_dispatch]
#[derive(Debug, Display)]
pub enum RoutingAgentsEnum {
    EpidemicRoutingAgent,
    FloodingRoutingAgent,
    SinkRoutingAgent,
    ExternalRoutingAgent,
    SprayAndWaitRoutingAgent,
    StaticRoutingAgent,
}

pub enum RoutingCmd {
    SenderForBundle(BundlePack, oneshot::Sender<(Vec<ClaSenderTask>, bool)>),
    Notify(RoutingNotifcation),
    Command(String),
    GetData(String, oneshot::Sender<String>),
    Shutdown,
}

/*
impl std::fmt::Display for RoutingAgentsEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
*/

#[async_trait]
#[enum_dispatch(RoutingAgentsEnum)]
pub trait RoutingAgent: Debug + Display {
    fn channel(&self) -> mpsc::Sender<RoutingCmd>;
}

pub fn routing_algorithms() -> Vec<&'static str> {
    vec![
        "epidemic",
        "flooding",
        "sink",
        "external",
        "sprayandwait",
        "static",
    ]
}

pub fn routing_options() -> Vec<&'static str> {
    vec!["sprayandwait.num_copies=<int>", "static.routes=<file>"]
}

pub fn new(routingagent: &str) -> RoutingAgentsEnum {
    match routingagent {
        "flooding" => FloodingRoutingAgent::new().into(),
        "epidemic" => EpidemicRoutingAgent::new().into(),
        "sink" => SinkRoutingAgent::new().into(),
        "static" => StaticRoutingAgent::new().into(),
        "external" => ExternalRoutingAgent::new().into(),
        "sprayandwait" => SprayAndWaitRoutingAgent::new().into(),
        _ => panic!("Unknown routing agent {}", routingagent),
    }
}
