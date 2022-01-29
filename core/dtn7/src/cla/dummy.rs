use std::collections::HashMap;

use super::{ConvergenceLayerAgent, HelpStr};
use async_trait::async_trait;
use bp7::ByteBuffer;
use dtn7_codegen::cla;
use log::debug;

#[cla(dummy)]
#[derive(Debug, Clone, Default, Copy)]
pub struct DummyConvergenceLayer {}

impl DummyConvergenceLayer {
    pub fn new(_local_settings: Option<&HashMap<String, String>>) -> DummyConvergenceLayer {
        DummyConvergenceLayer {}
    }
}
#[async_trait]
impl ConvergenceLayerAgent for DummyConvergenceLayer {
    async fn setup(&mut self) {}

    fn port(&self) -> u16 {
        0
    }
    fn name(&self) -> &'static str {
        // my_name() is generated from cla proc macro attribute
        self.my_name()
    }
    async fn scheduled_submission(&self, _dest: &str, _ready: &[ByteBuffer]) -> bool {
        debug!("Scheduled submission Dummy Conversion Layer");
        true
    }
}

impl HelpStr for DummyConvergenceLayer {}

impl std::fmt::Display for DummyConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "dummy")
    }
}
