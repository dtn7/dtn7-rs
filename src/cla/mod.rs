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
use strum::EnumIter;
use strum::IntoEnumIterator;
use tcp::TcpConvergenceLayer;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct ClaSender {
    pub remote: PeerAddress,
    pub port: Option<u16>,
    pub agent: ConvergenceLayerAgents,
}
impl ClaSender {
    /// Create new convergence layer agent just for sending bundles
    pub async fn transfer(&self, ready: &[ByteBuffer]) -> bool {
        let sender = new(&self.agent, None);
        let dest = if self.port.is_some() {
            format!("{}:{}", self.remote, self.port.unwrap())
        } else {
            self.remote.to_string()
        };
        sender.scheduled_submission(&dest, ready).await
    }
}

#[enum_dispatch]
#[derive(Debug, Display)]
pub enum CLAEnum {
    DummyConvergenceLayer,
    MtcpConvergenceLayer,
    HttpConvergenceLayer,
    TcpConvergenceLayer,
}

/*
impl std::fmt::Display for CLAEnum {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
*/

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

#[derive(Debug, Display, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, EnumIter)]
pub enum ConvergenceLayerAgents {
    DummyConvergenceLayer,
    MtcpConvergenceLayer,
    HttpConvergenceLayer,
    TcpConvergenceLayer,
}

impl ConvergenceLayerAgents {
    pub fn enumerate_help_str() -> String {
        let mut string = String::new();
        for variant in Self::iter() {
            string.push_str(variant.into());
            string.push_str(", ");
        }
        string
    }
    pub fn local_help_str() -> String {
        let mut string = String::new();
        for variant in Self::iter() {
            string.push('\n');
            string.push_str(variant.into());
            string.push(':');
            match variant {
                ConvergenceLayerAgents::DummyConvergenceLayer => {
                    string.push_str(dummy::DummyConvergenceLayer::local_help_str())
                }
                ConvergenceLayerAgents::MtcpConvergenceLayer => {
                    string.push_str(mtcp::MtcpConvergenceLayer::local_help_str())
                }
                ConvergenceLayerAgents::HttpConvergenceLayer => {
                    string.push_str(http::HttpConvergenceLayer::local_help_str())
                }
                ConvergenceLayerAgents::TcpConvergenceLayer => {
                    string.push_str(tcp::TcpConvergenceLayer::local_help_str())
                }
            }
        }
        string
    }
    pub fn global_help_str() -> String {
        let mut string = String::new();
        for variant in Self::iter() {
            string.push('\n');
            string.push_str(variant.into());
            string.push(':');
            match variant {
                ConvergenceLayerAgents::DummyConvergenceLayer => {
                    string.push_str(dummy::DummyConvergenceLayer::global_help_str())
                }
                ConvergenceLayerAgents::MtcpConvergenceLayer => {
                    string.push_str(mtcp::MtcpConvergenceLayer::global_help_str())
                }
                ConvergenceLayerAgents::HttpConvergenceLayer => {
                    string.push_str(http::HttpConvergenceLayer::global_help_str())
                }
                ConvergenceLayerAgents::TcpConvergenceLayer => {
                    string.push_str(tcp::TcpConvergenceLayer::global_help_str())
                }
            }
        }
        string
    }
}

impl From<ConvergenceLayerAgents> for &'static str {
    fn from(v: ConvergenceLayerAgents) -> Self {
        match v {
            ConvergenceLayerAgents::DummyConvergenceLayer => "dummy",
            ConvergenceLayerAgents::MtcpConvergenceLayer => "mtcp",
            ConvergenceLayerAgents::HttpConvergenceLayer => "http",
            ConvergenceLayerAgents::TcpConvergenceLayer => "tcp",
        }
    }
}

impl FromStr for ConvergenceLayerAgents {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "dummy" => Ok(Self::DummyConvergenceLayer),
            "mtcp" => Ok(Self::MtcpConvergenceLayer),
            "http" => Ok(Self::HttpConvergenceLayer),
            "tcp" => Ok(Self::TcpConvergenceLayer),
            _ => Err(format!("Unknown convergence layer agent agent {}", s)),
        }
    }
}

impl From<&str> for ConvergenceLayerAgents {
    fn from(s: &str) -> Self {
        Self::from_str(s).unwrap()
    }
}

// returns a new CLA for the corresponding string ("<CLA name>[:local_port]").
// Example usage: 'dummy', 'mtcp', 'mtcp:16161'
pub fn new(
    cla: &ConvergenceLayerAgents,
    local_settings: Option<&HashMap<String, String>>,
) -> CLAEnum {
    match cla {
        ConvergenceLayerAgents::DummyConvergenceLayer => dummy::DummyConvergenceLayer::new().into(),
        ConvergenceLayerAgents::MtcpConvergenceLayer => {
            mtcp::MtcpConvergenceLayer::new(local_settings).into()
        }
        ConvergenceLayerAgents::HttpConvergenceLayer => {
            http::HttpConvergenceLayer::new(local_settings).into()
        }
        ConvergenceLayerAgents::TcpConvergenceLayer => {
            tcp::TcpConvergenceLayer::new(local_settings).into()
        }
    }
}
