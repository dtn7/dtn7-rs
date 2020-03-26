use bp7::{CreationTimestamp, EndpointID};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DtnClient {
    localhost: String,
    port: u16,
}

impl DtnClient {
    pub fn new() -> Self {
        DtnClient {
            localhost: "127.0.0.1".into(),
            port: 3000,
        }
    }
    pub fn with_host_and_port(localhost: String, port: u16) -> Self {
        DtnClient { localhost, port }
    }
    pub fn local_node_id(&self) -> anyhow::Result<EndpointID> {
        Ok(attohttpc::get(&format!(
            "http://{}:{}/status/nodeid",
            self.localhost, self.port
        ))
        .send()?
        .text()?
        .try_into()?)
    }
    pub fn creation_timestamp(&self) -> anyhow::Result<CreationTimestamp> {
        let response = attohttpc::get(&format!("http://{}:{}/cts", self.localhost, self.port))
            .send()?
            .text()?;
        Ok(serde_json::from_str(&response)?)
    }
    pub fn register_application_endpoint(&self, path: &str) -> anyhow::Result<()> {
        let response = attohttpc::get(&format!(
            "http://{}:{}/register?{}",
            self.localhost, self.port, path
        ))
        .send()?
        .text()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WsSendData {
    pub src: EndpointID,
    pub dst: EndpointID,
    pub delivery_notification: bool,
    pub lifetime: Duration,
    pub data: Vec<u8>,
}
