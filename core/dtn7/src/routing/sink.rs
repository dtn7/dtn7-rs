use super::RoutingAgent;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct SinkRoutingAgent {
    tx: mpsc::Sender<super::RoutingCmd>,
}

impl Default for SinkRoutingAgent {
    fn default() -> Self {
        SinkRoutingAgent::new()
    }
}

impl SinkRoutingAgent {
    pub fn new() -> Self {
        let (tx, mut rx) = mpsc::channel(1);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::RoutingCmd::SenderForBundle(_bp, reply) => {
                        reply.send((vec![], false)).unwrap();
                    }
                    super::RoutingCmd::Shutdown => {
                        break;
                    }
                    super::RoutingCmd::Command(_cmd) => {}
                    super::RoutingCmd::GetData(_, tx) => {
                        tx.send("unimplemented!".to_string()).unwrap();
                    }
                    super::RoutingCmd::Notify(_) => {}
                }
            }
        });

        SinkRoutingAgent { tx }
    }
}

#[async_trait]
impl RoutingAgent for SinkRoutingAgent {
    fn channel(&self) -> Sender<crate::RoutingCmd> {
        self.tx.clone()
    }
}

impl std::fmt::Display for SinkRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SinkRoutingAgent")
    }
}
