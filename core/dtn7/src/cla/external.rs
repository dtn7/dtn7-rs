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
}

impl ExternalConvergenceLayer {
    pub fn new(local_settings: Option<&HashMap<String, String>>) -> ExternalConvergenceLayer {
        let settings = local_settings.expect("no settings for ECLA");

        let mut port: u16 = 0;
        if let Some(setting_port) = settings.get("port") {
            port = u16::from_str(setting_port.as_str()).unwrap();
        }

        let name = settings
            .get("name")
            .expect("name missing")
            .to_string()
            .clone();
        let (tx, mut rx) = mpsc::channel(1);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(dest, ready, reply) => {
                        reply
                            .send(scheduled_submission(
                                name.clone().as_str(),
                                dest.as_str(),
                                &ready,
                            ))
                            .unwrap();
                    }
                    super::ClaCmd::Shutdown => {
                        break;
                    }
                }
            }
        });

        ExternalConvergenceLayer { tx, name, port }
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
        return self.tx.clone();
    }
}

impl HelpStr for ExternalConvergenceLayer {}

impl std::fmt::Display for ExternalConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "external")
    }
}
