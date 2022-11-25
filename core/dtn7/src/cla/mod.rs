pub mod dummy;
pub mod ecla;
pub mod external;
pub mod http;
pub mod httppull;
pub mod mtcp;
pub mod tcp;

use self::http::HttpConvergenceLayer;
use anyhow::Result;
use async_trait::async_trait;
use bp7::{ByteBuffer, EndpointID};
use derive_more::*;
use dtn7_codegen::init_cla_subsystem;
use dummy::DummyConvergenceLayer;
use enum_dispatch::enum_dispatch;
use external::ExternalConvergenceLayer;
use httppull::HttpPullConvergenceLayer;
use mtcp::MtcpConvergenceLayer;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
};
use tcp::TcpConvergenceLayer;
use tokio::sync::{mpsc, oneshot};

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
