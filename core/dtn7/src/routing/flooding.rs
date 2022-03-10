use super::RoutingAgent;
use crate::routing::RoutingCmd;
use crate::PEERS;
use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

/// Simple flooding-basic routing.
/// All bundles are sent to all known peers again and again.
#[derive(Debug)]
pub struct FloodingRoutingAgent {
    tx: mpsc::Sender<super::RoutingCmd>,
}

impl FloodingRoutingAgent {
    pub fn new() -> FloodingRoutingAgent {
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::RoutingCmd::SenderForBundle(_bp, reply) => {
                        let mut clas = Vec::new();
                        for (_, p) in (*PEERS.lock()).iter() {
                            if let Some(cla) = p.first_cla() {
                                clas.push(cla);
                            }
                        }

                        tokio::spawn(async move {
                            reply.send((clas, false)).unwrap();
                        });
                    }
                    super::RoutingCmd::Shutdown => {
                        break;
                    }
                    super::RoutingCmd::Notify(_) => {}
                }
            }
        });

        FloodingRoutingAgent { tx }
    }
}
impl std::fmt::Display for FloodingRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "FloodingRoutingAgent")
    }
}

#[async_trait]
impl RoutingAgent for FloodingRoutingAgent {
    fn channel(&self) -> Sender<RoutingCmd> {
        return self.tx.clone();
    }
}
