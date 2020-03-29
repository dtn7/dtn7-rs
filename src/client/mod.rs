use bp7::{CreationTimestamp, EndpointID};
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("message not utf8: {0}")]
    NonUtf8(#[from] std::string::FromUtf8Error),
    #[error("serde cbor error: {0}")]
    Cbor(#[from] serde_cbor::Error),
    #[error("serde json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http connection error: {0}")]
    Http(#[from] attohttpc::Error),
    #[error("failed to create endpoint: {0}")]
    EndpointIdInvalid(#[from] bp7::eid::EndpointIdError),
}

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
    pub fn local_node_id(&self) -> Result<EndpointID, ClientError> {
        Ok(attohttpc::get(&format!(
            "http://{}:{}/status/nodeid",
            self.localhost, self.port
        ))
        .send()?
        .text()?
        .try_into()?)
    }
    pub fn creation_timestamp(&self) -> Result<CreationTimestamp, ClientError> {
        let response = attohttpc::get(&format!("http://{}:{}/cts", self.localhost, self.port))
            .send()?
            .text()?;
        Ok(serde_json::from_str(&response)?)
    }
    pub fn register_application_endpoint(&self, path: &str) -> Result<(), ClientError> {
        let _response = attohttpc::get(&format!(
            "http://{}:{}/register?{}",
            self.localhost, self.port, path
        ))
        .send()?
        .text()?;
        Ok(())
    }
    pub fn unregister_application_endpoint(&self, path: &str) -> Result<(), ClientError> {
        let _response = attohttpc::get(&format!(
            "http://{}:{}/unregister?{}",
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
