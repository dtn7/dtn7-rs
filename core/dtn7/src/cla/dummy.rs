use std::collections::HashMap;

use super::TransferResult;
use super::{ConvergenceLayerAgent, HelpStr};
use async_trait::async_trait;
use dtn7_codegen::cla;
use log::debug;
use tokio::sync::mpsc;

#[cla(dummy)]
#[derive(Debug, Clone)]
pub struct DummyConvergenceLayer {
    tx: mpsc::Sender<super::ClaCmd>,
}

impl DummyConvergenceLayer {
    pub fn new(_local_settings: Option<&HashMap<String, String>>) -> DummyConvergenceLayer {
        let (tx, mut rx) = mpsc::channel(1);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(remote, _, reply) => {
                        debug!(
                            "DummyConvergenceLayer: received transfer command for {}",
                            remote
                        );
                        reply.send(TransferResult::Successful).unwrap();
                    }
                    super::ClaCmd::Shutdown => {
                        debug!("DummyConvergenceLayer: received shutdown command");
                        break;
                    }
                }
            }
        });
        DummyConvergenceLayer { tx }
    }
}

#[async_trait]
impl ConvergenceLayerAgent for DummyConvergenceLayer {
    async fn setup(&mut self) {}

    fn port(&self) -> u16 {
        0
    }

    fn name(&self) -> &str {
        // my_name() is generated from cla proc macro attribute
        self.my_name()
    }

    fn channel(&self) -> tokio::sync::mpsc::Sender<super::ClaCmd> {
        self.tx.clone()
    }
}

impl HelpStr for DummyConvergenceLayer {}

impl std::fmt::Display for DummyConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "dummy")
    }
}
