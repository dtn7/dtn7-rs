use super::ConvergenceLayerAgent;
use async_trait::async_trait;
use std::collections::HashMap;

use crate::cla::ecla::processing::scheduled_submission;
use crate::cla::HelpStr;
use bp7::ByteBuffer;
use dtn7_codegen::cla;

#[cla(external)]
#[derive(Debug, Clone, Default)]
pub struct ExternalConvergenceLayer {
    name: String,
}

impl ExternalConvergenceLayer {
    pub fn new(local_settings: Option<&HashMap<String, String>>) -> ExternalConvergenceLayer {
        ExternalConvergenceLayer {
            name: local_settings
                .expect("no settings for ECLA")
                .get("name")
                .expect("name missing")
                .to_string(),
        }
    }
}

#[async_trait]
impl ConvergenceLayerAgent for ExternalConvergenceLayer {
    async fn setup(&mut self) {}
    fn port(&self) -> u16 {
        0
    }
    fn name(&self) -> &str {
        self.name.as_str()
    }
    fn local_settings(&self) -> Option<HashMap<String, String>> {
        let mut settings: HashMap<String, String> = HashMap::new();
        settings.insert("name".to_string(), self.name.clone());
        Some(settings)
    }
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
        return scheduled_submission(&self.name, dest, ready);
    }
}

impl HelpStr for ExternalConvergenceLayer {}

impl std::fmt::Display for ExternalConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "external")
    }
}
