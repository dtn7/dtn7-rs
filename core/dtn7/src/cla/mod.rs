pub mod dummy;
pub mod http;
pub mod mtcp;
pub mod tcp;

use self::http::HttpConvergenceLayer;
use crate::core::peer::PeerAddress;
use async_trait::async_trait;
use bp7::ByteBuffer;
use derive_more::*;
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

use dtn7_codegen::init_cla_subsystem;

// generate various helpers
// - enum CLAsAvailable for verification and loading from str
// - enum CLAEnum for actual implementations
// convergence_layer_agents()
// local_help()
// global_help()
init_cla_subsystem!();

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
}

#[async_trait]
#[enum_dispatch(CLAEnum)]
pub trait ConvergenceLayerAgent: Debug + Display {
    async fn setup(&mut self);
    fn port(&self) -> u16;
    fn name(&self) -> &'static str;
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool;
}

pub trait HelpStr {
    fn local_help_str() -> &'static str {
        "<>"
    }
    fn global_help_str() -> &'static str {
        "<>"
    }
}
