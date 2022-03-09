use super::RoutingAgent;
use crate::core::bundlepack::BundlePack;
use crate::ClaSenderTask;
use async_trait::async_trait;

#[derive(Default, Debug)]
pub struct SinkRoutingAgent;

impl SinkRoutingAgent {
    pub fn new() -> Self {
        SinkRoutingAgent {}
    }
}

#[async_trait]
impl RoutingAgent for SinkRoutingAgent {
    async fn sender_for_bundle(&mut self, _bp: &BundlePack) -> (Vec<ClaSenderTask>, bool) {
        (vec![], false)
    }
}

impl std::fmt::Display for SinkRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SinkRoutingAgent")
    }
}
