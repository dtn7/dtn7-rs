use super::ConvergencyLayerAgent;
use bp7::ByteBuffer;
use log::{debug, error, info, warn};

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

    fn scheduled_submission(&self, _ready: &[ByteBuffer], _dest: &String) {
        debug!("Scheduled submission Dummy Conversion Layer");
    }
}

impl std::fmt::Display for DummyConvergencyLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "DummyConversionLayer")
    }
}
