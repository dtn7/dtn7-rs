pub mod dummy;
pub mod ecla;
pub mod external;
pub mod http;
pub mod httppull;
pub mod mtcp;
pub mod tcp;
pub mod udp;

use self::http::HttpConvergenceLayer;
use anyhow::{Context, Result};
use async_trait::async_trait;
use bp7::{ByteBuffer, EndpointID};
use dtn7_codegen::init_cla_subsystem;
use dummy::DummyConvergenceLayer;
use enum_dispatch::enum_dispatch;
use external::ExternalConvergenceLayer;
use httppull::HttpPullConvergenceLayer;
use mtcp::MtcpConvergenceLayer;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::str::FromStr;
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
};
use tcp::TcpConvergenceLayer;
use tokio::sync::{mpsc, oneshot};
use udp::UdpConvergenceLayer;

// generate various helpers
// - enum CLAsAvailable for verification and loading from str
// - enum CLAEnum for actual implementations
// convergence_layer_agents()
// local_help()
// global_help()
init_cla_subsystem!();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferResult {
    Successful,
    Failure,
}

#[derive(Debug)]
pub enum ClaCmd {
    Transfer(String, ByteBuffer, oneshot::Sender<TransferResult>),
    Shutdown,
}

/// Resolves a transfer destination such as `10.0.0.1:4556`, `[fe80::1]:4556` or
/// `node1.example.org:4556` to a socket address.
pub async fn resolve_destination(dest: &str) -> Result<SocketAddr> {
    tokio::net::lookup_host(dest)
        .await
        .with_context(|| format!("could not resolve {}", dest))?
        .next()
        .with_context(|| format!("no address found for {}", dest))
}

#[derive(Debug, Clone)]
pub struct ClaSenderTask {
    pub tx: mpsc::Sender<ClaCmd>,
    pub dest: String,
    pub cla_name: String,
    pub next_hop: EndpointID,
}

impl ClaSenderTask {
    pub async fn transfer(&self, ready: ByteBuffer) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = ClaCmd::Transfer(self.dest.clone(), ready, reply_tx);
        self.tx.send(cmd).await?;
        if reply_rx.await? == TransferResult::Failure {
            return Err(anyhow::anyhow!(
                "CLA {} failed to send bundle",
                self.cla_name
            ));
        }
        Ok(())
    }
}

// enum_dispatch copies the #[must_use] that async_trait adds to async methods
#[allow(clippy::double_must_use)]
#[async_trait]
#[enum_dispatch(CLAEnum)]
pub trait ConvergenceLayerAgent: Debug + Display {
    async fn setup(&mut self);
    fn port(&self) -> u16;
    fn name(&self) -> &str;
    fn local_settings(&self) -> Option<HashMap<String, String>> {
        None
    }
    fn channel(&self) -> mpsc::Sender<ClaCmd>;
    fn accepting(&self) -> bool {
        true
    }
}

pub trait HelpStr {
    fn local_help_str() -> &'static str {
        "<>"
    }
    fn global_help_str() -> &'static str {
        "<>"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn destinations_are_resolved() {
        let v4: SocketAddr = "127.0.0.1:4556".parse().unwrap();
        let v6: SocketAddr = "[::1]:4556".parse().unwrap();
        assert_eq!(resolve_destination("127.0.0.1:4556").await.unwrap(), v4);
        assert_eq!(resolve_destination("[::1]:4556").await.unwrap(), v6);
        assert!(resolve_destination("localhost:4556").await.is_ok());
        assert!(resolve_destination("no port").await.is_err());
    }
}
