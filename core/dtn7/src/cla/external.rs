use super::ConvergenceLayerAgent;
use crate::cla::ecla::processing::scheduled_submission;
use crate::cla::{ClaCmd, HelpStr};
use async_trait::async_trait;
use dtn7_codegen::cla;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

#[cla(external)]
#[derive(Debug, Clone)]
pub struct ExternalConvergenceLayer {
    tx: mpsc::Sender<super::ClaCmd>,
    name: String,
    port: u16,
    discovery_only: bool,
}

impl ExternalConvergenceLayer {
    pub fn new(local_settings: Option<&HashMap<String, String>>) -> ExternalConvergenceLayer {
        let settings = local_settings.expect("no settings for ECLA");

        let mut port: u16 = 0;
        if let Some(setting_port) = settings.get("port") {
            port = u16::from_str(setting_port.as_str()).unwrap();
        }
        let mut discovery_only: bool = false;
        if let Some(discovery_only_str) = settings.get("discovery_only") {
            discovery_only = bool::from_str(discovery_only_str.as_str()).unwrap();
        }

        let name = settings.get("name").expect("name missing").to_string();
        let task_name = name.clone();
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(dest, ready, reply) => {
                        if !discovery_only {
                            let name = task_name.clone();
                            tokio::spawn(async move {
                                reply
                                    .send(scheduled_submission(name, dest, &ready))
                                    .unwrap();
                            });
                        } else {
                            reply.send(super::TransferResult::Failure).unwrap();
                        }
                    }
                    super::ClaCmd::Shutdown => {
                        break;
                    }
                }
            }
        });

        ExternalConvergenceLayer {
            tx,
            name,
            port,
            discovery_only,
        }
    }
}

#[async_trait]
impl ConvergenceLayerAgent for ExternalConvergenceLayer {
    async fn setup(&mut self) {}
    fn port(&self) -> u16 {
        self.port
    }
    fn name(&self) -> &str {
        self.name.as_str()
    }
    fn local_settings(&self) -> Option<HashMap<String, String>> {
        let mut settings: HashMap<String, String> = HashMap::new();
        settings.insert("name".to_string(), self.name.clone());
        Some(settings)
    }
    fn channel(&self) -> Sender<ClaCmd> {
        self.tx.clone()
    }
    fn accepting(&self) -> bool {
        !self.discovery_only
    }
}

impl HelpStr for ExternalConvergenceLayer {
    fn local_help_str() -> &'static str {
        "port=1234:discovery_only=false"
    }
}

impl std::fmt::Display for ExternalConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "external")
    }
}
