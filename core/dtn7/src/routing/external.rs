use super::erouting::processing::{notify, sender_for_bundle};
use super::RoutingAgent;
use crate::routing::RoutingCmd;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct ExternalRoutingAgent {
    tx: mpsc::Sender<super::RoutingCmd>,
}

impl Default for ExternalRoutingAgent {
    fn default() -> Self {
        ExternalRoutingAgent::new()
    }
}

impl ExternalRoutingAgent {
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::RoutingCmd::SenderForBundle(bp, reply) => {
                        tokio::spawn(async move {
                            reply.send(sender_for_bundle(&bp).await).unwrap();
                        });
                    }
                    super::RoutingCmd::Shutdown => {
                        break;
                    }
                    super::RoutingCmd::Notify(notification) => {
                        notify(notification);
                    }
                }
            }
        });

        ExternalRoutingAgent { tx }
    }
}

#[async_trait]
impl RoutingAgent for ExternalRoutingAgent {
    fn channel(&self) -> Sender<RoutingCmd> {
        self.tx.clone()
    }
}

impl std::fmt::Display for ExternalRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ExternalRoutingAgent")
    }
}
