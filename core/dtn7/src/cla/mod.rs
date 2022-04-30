pub mod dummy;
pub mod http;
pub mod mtcp;
pub mod tcp;

use crate::core::PeerType;
use crate::PEERS;

use self::http::HttpConvergenceLayer;
use anyhow::Result;
use async_trait::async_trait;
use bp7::{ByteBuffer, EndpointID};
use derive_more::*;
use dtn7_codegen::init_cla_subsystem;
use dummy::DummyConvergenceLayer;
use enum_dispatch::enum_dispatch;
use log::debug;
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

#[derive(Debug, Clone, PartialEq)]
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
    pub cla: CLAEnum,
    pub next_hop: EndpointID,
    pub dest: String,
}

impl ClaSenderTask {
    pub async fn transfer(&self, ready: ByteBuffer) -> Result<()> {
        self.cla
            .send(self.dest.clone(), ready, self.next_hop.clone())
            .await
    }
}

#[async_trait]
#[enum_dispatch(CLAEnum)]
pub trait ConvergenceLayerAgent: Debug + Display + Clone {
    async fn setup(&mut self);
    fn port(&self) -> u16;
    fn name(&self) -> &'static str;
    fn channel(&self) -> mpsc::Sender<ClaCmd>;
    async fn send(&self, node: String, ready: ByteBuffer, next_hop: EndpointID) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let cmd = ClaCmd::Transfer(node, ready, reply_tx);
        self.channel().send(cmd).await?;
        if reply_rx.await? == TransferResult::Failure {
            let mut failed_peer = None;
            if let Some(peer_entry) = (*PEERS.lock()).get_mut(&next_hop.node().unwrap()) {
                debug!(
                    "Reporting failed sending to peer: {}",
                    &next_hop.node().unwrap()
                );
                peer_entry.report_fail();
                if peer_entry.failed_too_much() && peer_entry.con_type == PeerType::Dynamic {
                    failed_peer = Some(peer_entry.node_name());
                }
            }
            if let Some(peer) = failed_peer {
                let peers_before = (*PEERS.lock()).len();
                (*PEERS.lock()).remove(&peer);
                let peers_after = (*PEERS.lock()).len();
                debug!("Removing peer {} from list of neighbors due to too many failed transmissions ({}/{})", peer, peers_before, peers_after);
            }
            return Err(anyhow::anyhow!("CLA {} failed to send bundle", self.name()));
        }
        Ok(())
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
