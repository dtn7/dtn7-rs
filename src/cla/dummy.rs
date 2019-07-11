use super::ConvergencyLayerAgent;
use bp7::ByteBuffer;
use log::{debug};

#[derive(Debug, Clone, Default)]
pub struct DummyConvergencyLayer {
    counter: u64,
}

impl DummyConvergencyLayer {
    pub fn new() -> DummyConvergencyLayer {
        DummyConvergencyLayer { counter: 0 }
    }
}
impl ConvergencyLayerAgent for DummyConvergencyLayer {
    fn setup(&mut self) {}

    fn port(&self) -> u16 {
        0
    }
    fn scheduled_submission(&self, _dest: &str, _ready: &[ByteBuffer]) -> bool{
        debug!("Scheduled submission Dummy Conversion Layer");
        true
    }
}

impl std::fmt::Display for DummyConvergencyLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "dummy")
    }
}
