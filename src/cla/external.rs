use super::ConvergenceLayerAgent;
use crate::cla::ecla::processing::scheduled_submission;
use async_trait::async_trait;
use bp7::ByteBuffer;
use std::fmt::Formatter;

#[derive(Clone, Default)]
pub struct ExternalConvergenceLayer {
    name: String,
}

impl std::fmt::Debug for ExternalConvergenceLayer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ExternalConvergenceLayer:{}", self.name)
    }
}

impl ExternalConvergenceLayer {
    pub fn new(name: String) -> ExternalConvergenceLayer {
        ExternalConvergenceLayer { name }
    }
}

#[async_trait]
impl ConvergenceLayerAgent for ExternalConvergenceLayer {
    async fn setup(&mut self) {}

    fn port(&self) -> u16 {
        return 0;
    }
    fn name(&self) -> &str {
        return self.name.as_str();
    }
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
        return scheduled_submission(&self.name, dest, ready);
    }
}

impl std::fmt::Display for ExternalConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "external")
    }
}
