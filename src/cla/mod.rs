pub mod dummy;
pub mod http;
pub mod mtcp;
pub mod tcp;
pub mod tcpcl;

use async_trait::async_trait;
use bp7::ByteBuffer;
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

#[async_trait]
pub trait ConvergenceLayerAgent: Debug + Send + Sync + Display {
    async fn setup(&mut self);
    fn port(&self) -> u16;
    fn name(&self) -> &'static str;
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool;
}

pub fn convergence_layer_agents() -> Vec<&'static str> {
    vec!["dummy", "mtcp", "http", "tcp"]
}

// returns a new CLA for the corresponding string ("<CLA name>[:local_port]").
// Example usage: 'dummy', 'mtcp', 'mtcp:16161'
pub fn new(cla_str: &str) -> Box<dyn ConvergenceLayerAgent> {
    let cla: Vec<&str> = cla_str.split(':').collect();
    let port: Option<u16> = cla.get(1).unwrap_or(&"-1").parse::<u16>().ok();
    match cla[0] {
        "dummy" => Box::new(dummy::DummyConvergenceLayer::new()),
        "mtcp" => Box::new(mtcp::MtcpConvergenceLayer::new(port)),
        "http" => Box::new(http::HttpConvergenceLayer::new(port)),
        "tcp" => Box::new(tcp::TcpConvergenceLayer::new(port)),
        _ => panic!("Unknown convergence layer agent agent {}", cla[0]),
    }
}
