use super::ConvergenceLayerAgent;
use async_trait::async_trait;
use bp7::ByteBuffer;
use log::debug;

#[derive(Debug, Clone, Default, Copy)]
pub struct DummyConvergenceLayer {
    counter: u64,
}

impl DummyConvergenceLayer {
    pub fn new() -> DummyConvergenceLayer {
        DummyConvergenceLayer { counter: 0 }
    }
}
#[async_trait]
impl ConvergenceLayerAgent for DummyConvergenceLayer {
    async fn setup(&mut self) {}

    fn port(&self) -> u16 {
        0
    }
    fn name(&self) -> &'static str {
        "dummy"
    }
    async fn scheduled_submission(&self, _dest: &str, _ready: &[ByteBuffer]) -> bool {
        debug!("Scheduled submission Dummy Conversion Layer");
        true
    }
}

impl std::fmt::Display for DummyConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "dummy")
    }
}
