pub mod dummy;

pub mod stcp;

use bp7::ByteBuffer;
use std::fmt::{Debug, Display};

pub trait ConvergencyLayerAgent: Debug + Send + Display {
    fn setup(&mut self);
    fn scheduled_submission(&self, ready: &[ByteBuffer], dest: &String);
}

pub fn convergency_layer_agents() -> Vec<&'static str> {
    vec!["dummy", "stcp"]
}

pub fn new(cla: &str) -> Box<ConvergencyLayerAgent> {
    match cla {
        "dummy" => Box::new(dummy::DummyConvergencyLayer::new()),
        "stcp" => Box::new(stcp::StcpConversionLayer::new()),
        _ => panic!("Unknown convergency layer agent agent {}", cla),
    }
}
