use super::ConvergencyLayerAgent;
use async_trait::async_trait;
use bp7::ByteBuffer;
use log::debug;

#[derive(Debug, Clone, Default, Copy)]
pub struct DummyConvergencyLayer {
    counter: u64,
}

impl DummyConvergencyLayer {
    pub fn new() -> DummyConvergencyLayer {
        DummyConvergencyLayer { counter: 0 }
    }
}
#[async_trait]
impl ConvergencyLayerAgent for DummyConvergencyLayer {
    async fn setup(&mut self) {}

    fn port(&self) -> u16 {
        0
    }
    fn name(&self) -> &'static str {
        "dummy"
    }
    fn scheduled_submission(&self, _dest: &str, _ready: &[ByteBuffer]) -> bool {
        debug!("Scheduled submission Dummy Conversion Layer");
        true
    }
}

impl std::fmt::Display for DummyConvergencyLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "dummy")
    }
}
