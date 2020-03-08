use crate::core::DtnPeer;
use bp7::EndpointID;
use config::{Config, File};
use log::debug;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Default, Clone, Serialize)]
pub struct DtnConfig {
    pub debug: bool,
    pub v4: bool,
    pub v6: bool,
    pub nodeid: String,
    pub host_eid: EndpointID,
    pub webport: u16,
    pub announcement_interval: Duration,
    pub janitor_interval: Duration,
    pub endpoints: Vec<String>,
    pub clas: Vec<String>,
    pub routing: String,
    pub peer_timeout: Duration,
    pub statics: Vec<DtnPeer>,
}

pub fn rnd_node_name() -> String {
    thread_rng().sample_iter(&Alphanumeric).take(10).collect()
}

impl From<PathBuf> for DtnConfig {
    fn from(item: PathBuf) -> Self {
        let mut dtncfg = DtnConfig::new();
        let mut s = Config::default();

        debug!("Loading config: {}", item.to_str().unwrap());

        // Start off by merging in the "default" configuration file
        s.merge(File::new(item.to_str().unwrap(), config::FileFormat::Toml))
            .unwrap();
        dtncfg.debug = s.get_bool("debug").unwrap_or(false);
        if dtncfg.debug {
            //std::env::set_var("RUST_LOG", "dtn7=debug,dtnd=debug");
        }
        dtncfg.v4 = s.get_bool("ipv4").unwrap_or(true);
        debug!("ipv4: {:?}", dtncfg.v4);
        dtncfg.v6 = s.get_bool("ipv6").unwrap_or(false);
        debug!("ipv6: {:?}", dtncfg.v6);

        debug!("debug: {:?}", dtncfg.debug);
        dtncfg.nodeid = s.get_str("nodeid").unwrap_or(dtncfg.nodeid);
        debug!("nodeid: {:?}", dtncfg.nodeid);

        dtncfg.routing = s.get_str("routing").unwrap_or(dtncfg.routing);
        debug!("routing: {:?}", dtncfg.routing);

        dtncfg.webport = s
            .get_int("webport")
            .unwrap_or_else(|_| i64::from(dtncfg.webport)) as u16;
        debug!("webport: {:?}", dtncfg.webport);

        dtncfg.janitor_interval = if let Ok(interval) = s.get_str("core.janitor") {
            humantime::parse_duration(&interval).unwrap_or_else(|_| Duration::new(0, 0))
        } else {
            dtncfg.janitor_interval
        };
        debug!("janitor: {:?}", dtncfg.janitor_interval);

        dtncfg.announcement_interval = if let Ok(interval) = s.get_str("discovery.interval") {
            humantime::parse_duration(&interval).unwrap_or_else(|_| Duration::new(0, 0))
        } else {
            dtncfg.announcement_interval
        };
        debug!("discovery-interval: {:?}", dtncfg.announcement_interval);

        dtncfg.peer_timeout = if let Ok(interval) = s.get_str("discovery.peer-timeout") {
            humantime::parse_duration(&interval).unwrap_or_else(|_| Duration::new(0, 0))
        } else {
            dtncfg.peer_timeout
        };
        debug!("discovery-peer-timeout: {:?}", dtncfg.peer_timeout);

        if let Ok(peers) = s.get_array("statics.peers") {
            for m in peers.iter() {
                let peer: DtnPeer =
                    crate::core::helpers::parse_peer_url(&m.clone().into_str().unwrap());
                debug!("Peer: {:?}", peer);
                dtncfg.statics.push(peer);
            }
        }
        if let Ok(endpoints) = s.get_table("endpoints.local") {
            for (_k, v) in endpoints.iter() {
                let eid = v.clone().into_str().unwrap();
                debug!("EID: {:?}", eid);
                dtncfg.endpoints.push(eid);
            }
        }
        if let Ok(clas) = s.get_table("convergencylayers.cla") {
            for (_k, v) in clas.iter() {
                let tab = v.clone().into_table().unwrap();
                let cla_id = tab["id"].clone().into_str().unwrap();
                let cla_port = if tab.contains_key("port") {
                    tab["port"].clone().into_int().unwrap_or(0) as u16
                } else {
                    0
                };
                if crate::cla::convergency_layer_agents().contains(&cla_id.as_str()) {
                    debug!("CLA: {:?}", cla_id);
                    dtncfg.clas.push(format!("{}:{}", cla_id, cla_port));
                }
            }
        }
        dtncfg
    }
}

impl DtnConfig {
    pub fn new() -> DtnConfig {
        let node_rnd: String = rnd_node_name();
        DtnConfig {
            debug: false,
            v4: true,
            v6: false,
            nodeid: node_rnd.clone(),
            host_eid: format!("dtn://{}", node_rnd).into(),
            announcement_interval: "2s".parse::<humantime::Duration>().unwrap().into(),
            webport: 3000,
            janitor_interval: "10s".parse::<humantime::Duration>().unwrap().into(),
            endpoints: Vec::new(),
            clas: Vec::new(),
            routing: "epidemic".into(),
            peer_timeout: "20s".parse::<humantime::Duration>().unwrap().into(),
            statics: Vec::new(),
        }
    }
    pub fn set(&mut self, cfg: DtnConfig) {
        self.debug = cfg.debug;
        self.v4 = cfg.v4;
        self.v6 = cfg.v6;
        self.nodeid = cfg.nodeid.clone();
        self.host_eid = format!("dtn://{}", cfg.nodeid).into();
        self.webport = cfg.webport;
        self.announcement_interval = cfg.announcement_interval;
        self.janitor_interval = cfg.janitor_interval;
        self.endpoints = cfg.endpoints;
        self.clas = cfg.clas;
        self.routing = cfg.routing;
        self.peer_timeout = cfg.peer_timeout;
        self.statics = cfg.statics;
    }
}
