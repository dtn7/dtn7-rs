pub mod dummy;
pub mod http;
pub mod mtcp;

use self::http::HttpConvergenceLayer;
use async_trait::async_trait;
use bp7::ByteBuffer;
use derive_more::*;
use dummy::DummyConvergenceLayer;
use enum_dispatch::enum_dispatch;
use mtcp::MtcpConvergenceLayer;
use std::fmt::{Debug, Display};
use std::net::IpAddr;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ClaSender {
    pub remote: IpAddr,
    pub port: Option<u16>,
    pub agent: String,
}
impl ClaSender {
    pub async fn transfer(&self, ready: &[ByteBuffer]) -> bool {
        let sender = new(&self.agent); // since we are not listening sender port is irrelevant
        let dest = if self.port.is_some() {
            format!("{}:{}", self.remote, self.port.unwrap())
        } else {
            self.remote.to_string()
        };
        sender.scheduled_submission(&dest, ready).await
    }
}

#[enum_dispatch]
#[derive(Debug, Display)]
pub enum CLAEnum {
    DummyConvergenceLayer,
    MtcpConvergenceLayer,
    HttpConvergenceLayer,
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
    fn name(&self) -> &'static str;
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool;
}

pub fn convergence_layer_agents() -> Vec<&'static str> {
    vec!["dummy", "mtcp", "http"]
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
        _ => panic!("Unknown convergence layer agent agent {}", cla[0]),
    }
}
