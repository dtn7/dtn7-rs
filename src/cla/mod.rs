pub mod dummy;
pub mod external;
pub mod http;
pub mod mtcp;

use self::http::HttpConvergenceLayer;
use crate::cla_names;
use async_trait::async_trait;
use bp7::ByteBuffer;
use derive_more::*;
use dummy::DummyConvergenceLayer;
use enum_dispatch::enum_dispatch;
use external::ExternalConvergenceLayer;
use mtcp::MtcpConvergenceLayer;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Display};
use std::net::IpAddr;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum RemoteAddr {
    IP(IpAddr),
    IPPort((IpAddr, u16)),
    Str(String),
}

impl std::fmt::Display for RemoteAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut dest = String::new();
        match self {
            RemoteAddr::IP(ip) => {
                dest.push_str(format!("{}", ip).as_str());
            }
            RemoteAddr::IPPort(ip_port) => {
                dest.push_str(format!("{}:{}", ip_port.0, ip_port.1).as_str());
            }
            RemoteAddr::Str(val) => {
                dest.push_str(val);
            }
        }
        return write!(f, "{}", dest);
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ClaSender {
    pub remote: RemoteAddr,
    pub agent: String,
}
impl ClaSender {
    pub async fn transfer(&self, ready: &[ByteBuffer]) -> bool {
        let sender = new(&self.agent); // since we are not listening sender port is irrelevant

        let mut dest = String::new();
        match &self.remote {
            RemoteAddr::IP(ip) => {
                dest.push_str(format!("{}", ip).as_str());
            }
            RemoteAddr::IPPort(ip_port) => {
                dest.push_str(format!("{}:{}", ip_port.0, ip_port.1).as_str());
            }
            RemoteAddr::Str(val) => {
                dest.push_str(val);
            }
        }

        sender.scheduled_submission(&dest, ready).await
    }
}

#[enum_dispatch]
#[derive(Debug, Display)]
pub enum CLAEnum {
    DummyConvergenceLayer,
    MtcpConvergenceLayer,
    HttpConvergenceLayer,
    ExternalConvergenceLayer,
}

/*
impl std::fmt::Display for CLAEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
*/

#[async_trait]
#[enum_dispatch(CLAEnum)]
pub trait ConvergenceLayerAgent: Debug + Display {
    async fn setup(&mut self);
    fn port(&self) -> u16;
    fn name(&self) -> &str;
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool;
}

pub fn convergence_layer_agents() -> Vec<&'static str> {
    vec!["dummy", "mtcp", "http", "external"]
}

// returns a new CLA for the corresponding string ("<CLA name>[:local_port]").
// Example usage: 'dummy', 'mtcp', 'mtcp:16161'
pub fn new(cla_str: &str) -> CLAEnum {
    let cla: Vec<&str> = cla_str.split(':').collect();
    let port: Option<u16> = cla.get(1).unwrap_or(&"-1").parse::<u16>().ok();

    match cla[0] {
        "dummy" => dummy::DummyConvergenceLayer::new().into(),
        "mtcp" => mtcp::MtcpConvergenceLayer::new(port).into(),
        "http" => http::HttpConvergenceLayer::new(port).into(),
        //"external" => external::ExternalConvergenceLayer::new(port).into(),
        _ => {
            // If CLA list contains a CLA name that is not from the static ones (dummy, mtcp, http) it MUST be a external one
            if cla_names().contains(&cla[0].to_string()) {
                return external::ExternalConvergenceLayer::new(cla[0].to_string()).into();
            }
            panic!("Unknown convergence layer agent agent {}", cla[0])
        }
    }
}
