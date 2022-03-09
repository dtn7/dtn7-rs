use super::erouting::processing::{notify, sender_for_bundle};
use super::RoutingAgent;
use crate::core::bundlepack::BundlePack;
use crate::{ClaSenderTask, RoutingNotifcation};
use async_trait::async_trait;

#[derive(Default, Debug)]
pub struct ExternalRoutingAgent;

impl ExternalRoutingAgent {
    pub fn new() -> Self {
        ExternalRoutingAgent {}
    }
}

#[async_trait]
impl RoutingAgent for ExternalRoutingAgent {
    fn notify(&mut self, notification: RoutingNotifcation) {
        notify(notification);
    }
    async fn sender_for_bundle(&mut self, bp: &BundlePack) -> (Vec<ClaSenderTask>, bool) {
        sender_for_bundle(bp).await
    }
}

impl std::fmt::Display for ExternalRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ExternalRoutingAgent")
    }
}
