pub mod dummy;
pub mod mtcp;

use bp7::ByteBuffer;
use std::fmt::{Debug, Display};
use std::net::IpAddr;

#[derive(Debug)]
pub struct CLA_sender {
    pub remote: IpAddr,
    pub port: Option<u16>,
    pub agent: String,
}
impl CLA_sender {
    pub fn transfer(&self, ready: &[ByteBuffer]) -> bool {
        let sender = new(&self.agent); // since we are not listening sender port is irrelevant
        let dest = if self.port.is_some() {
            format!("{}:{}", self.remote, self.port.unwrap())
        } else {
            self.remote.to_string()
        };
        sender.scheduled_submission(&dest, ready)
    }
}

pub trait ConvergencyLayerAgent: Debug + Send + Display {
    fn setup(&mut self);
    fn port(&self) -> u16;
    fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool;
}

pub fn convergency_layer_agents() -> Vec<&'static str> {
    vec!["dummy", "mtcp"]
}

// returns a new CLA for the corresponding string ("<CLA name>[:local_port]").
// Example usage: 'dummy', 'mtcp', 'mtcp:16161'
pub fn new(cla_str: &str) -> Box<ConvergencyLayerAgent> {
    let cla: Vec<&str> = cla_str.split(':').collect();
    let port: Option<u16> = cla.get(1).unwrap_or(&"-1").parse::<u16>().ok();
    match cla[0] {
        "dummy" => Box::new(dummy::DummyConvergencyLayer::new()),
        "mtcp" => Box::new(mtcp::MtcpConversionLayer::new(port)),
        _ => panic!("Unknown convergency layer agent agent {}", cla[0]),
    }
}
