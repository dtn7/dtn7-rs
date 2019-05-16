pub mod dummy;

pub mod mtcp;
pub mod stcp;

use bp7::ByteBuffer;
use std::fmt::{Debug, Display};

pub trait ConvergencyLayerAgent: Debug + Send + Display {
    fn setup(&mut self);
    fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]);
}

pub fn convergency_layer_agents() -> Vec<&'static str> {
    vec!["dummy", "stcp", "mtcp"]
}

pub fn new(cla: &str) -> Box<ConvergencyLayerAgent> {
    match cla {
        "dummy" => Box::new(dummy::DummyConvergencyLayer::new()),
        "stcp" => Box::new(stcp::StcpConversionLayer::new()),
        "mtcp" => Box::new(mtcp::MtcpConversionLayer::new()),
        _ => panic!("Unknown convergency layer agent agent {}", cla),
    }
}
