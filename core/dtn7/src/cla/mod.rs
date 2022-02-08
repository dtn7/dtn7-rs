pub mod dummy;
pub mod http;
pub mod mtcp;
pub mod tcp;

use self::http::HttpConvergenceLayer;
use async_trait::async_trait;
use bp7::{ByteBuffer, EndpointID};
use derive_more::*;
use dtn7_codegen::init_cla_subsystem;
use dummy::DummyConvergenceLayer;
use enum_dispatch::enum_dispatch;
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

#[derive(Debug)]
pub enum ClaCmd {
    Transfer(String, ByteBuffer, oneshot::Sender<bool>),
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
    pub async fn transfer(&self, ready: ByteBuffer) -> bool {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = ClaCmd::Transfer(self.dest.clone(), ready, reply_tx);
        self.tx.send(cmd).await.unwrap();
        reply_rx.await.unwrap()
    }
}
/*
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ClaSender {
    pub remote: PeerAddress,
    pub port: Option<u16>,
    pub agent: CLAsAvailable,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ClaSender {
    pub remote: PeerAddress,
    pub port: Option<u16>,
    pub agent: CLAsAvailable,
}
impl ClaSender {
    /// Create new convergence layer agent just for sending bundles
    pub async fn transfer(&self, ready: &[ByteBuffer]) -> bool {
        // TODO: provide proper local settings
        let sender = new(&self.agent, None);
        let dest = if self.port.is_some() {
            format!("{}:{}", self.remote, self.port.unwrap())
        } else {
            self.remote.to_string()
        };
        sender.scheduled_submission(&dest, ready).await
    }
}*/

#[async_trait]
#[enum_dispatch(CLAEnum)]
pub trait ConvergenceLayerAgent: Debug + Display {
    async fn setup(&mut self);
    fn port(&self) -> u16;
    fn name(&self) -> &'static str;
    fn channel(&self) -> mpsc::Sender<ClaCmd>;
}

pub trait HelpStr {
    fn local_help_str() -> &'static str {
        "<>"
    }
    fn global_help_str() -> &'static str {
        "<>"
    }
}
