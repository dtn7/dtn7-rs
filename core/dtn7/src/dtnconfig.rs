use crate::cla::CLAsAvailable;
use crate::core::DtnPeer;
use bp7::EndpointID;
use config::{Config, File};
use log::{debug, error};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::Serialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::{convert::TryInto, time::Duration};

#[derive(Debug, Default, Clone, Serialize)]
pub struct DtnConfig {
    pub debug: bool,
    pub unsafe_httpd: bool,
    pub v4: bool,
    pub v6: bool,
    pub custom_timeout: bool,
    pub enable_period: bool,
    pub nodeid: String,
    pub host_eid: EndpointID,
    pub webport: u16,
    pub announcement_interval: Duration,
    pub discovery_destinations: HashMap<String, u32>,
    pub janitor_interval: Duration,
    pub endpoints: Vec<String>,
    pub clas: Vec<(CLAsAvailable, HashMap<String, String>)>,
    pub cla_global_settings: HashMap<CLAsAvailable, HashMap<String, String>>,
    pub services: HashMap<u8, String>,
    pub routing: String,
    pub peer_timeout: Duration,
    pub statics: Vec<DtnPeer>,
    pub workdir: PathBuf,
    pub db: String,
    pub generate_status_reports: bool,
}

pub fn rnd_node_name() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect()
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
        dtncfg.generate_status_reports = s.get_bool("generate_status_reports").unwrap_or(false);
        dtncfg.unsafe_httpd = s.get_bool("unsafe_httpd").unwrap_or(false);
        dtncfg.v4 = s.get_bool("ipv4").unwrap_or(true);
        debug!("ipv4: {:?}", dtncfg.v4);
        dtncfg.v6 = s.get_bool("ipv6").unwrap_or(false);
        debug!("ipv6: {:?}", dtncfg.v6);
        dtncfg.enable_period = s.get_bool("beacon-period").unwrap_or(false);
        debug!("announcing period: {:?}", dtncfg.enable_period);
        debug!("debug: {:?}", dtncfg.debug);
        let nodeid = s.get_str("nodeid").unwrap_or_else(|_| rnd_node_name());
        if nodeid.chars().all(char::is_alphanumeric) {
            dtncfg.host_eid = if let Ok(number) = nodeid.parse::<u64>() {
                format!("ipn:{}.0", number).try_into().unwrap()
            } else {
                format!("dtn://{}", nodeid).try_into().unwrap()
            };
        } else {
            dtncfg.host_eid = nodeid.try_into().unwrap();
            if !dtncfg.host_eid.is_node_id() {
                panic!("Invalid node id!");
            }
        }
        debug!("nodeid: {:?}", dtncfg.host_eid);
        dtncfg.nodeid = dtncfg.host_eid.to_string();

        dtncfg.routing = s.get_str("routing").unwrap_or(dtncfg.routing);
        debug!("routing: {:?}", dtncfg.routing);

        dtncfg.workdir = if let Ok(wd) = s.get_str("workdir") {
            PathBuf::from(wd)
        } else {
            std::env::current_dir().unwrap()
        };
        debug!("workdir: {:?}", dtncfg.workdir);

        dtncfg.db = s.get_str("db").unwrap_or_else(|_| "mem".into());
        debug!("db: {:?}", dtncfg.db);

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
                let mut tab = v.clone().into_table().unwrap();
                let cla_id = tab.remove("id").unwrap().into_str().unwrap();
                match CLAsAvailable::from_str(cla_id.as_str()) {
                    Ok(agent) => {
                        debug!("CLA: {:?}", cla_id);
                        let mut local_settings = HashMap::new();
                        for (k, v) in tab {
                            local_settings.insert(k, v.into_str().unwrap());
                        }
                        dtncfg.clas.push((agent, local_settings));
                    }
                    Err(message) => {
                        error!("Error parsing cla config: {}", message)
                    }
                }
            }
        }
        if let Ok(cla_global_settings) = s.get_table("convergencylayers.global") {
            for (k, v) in cla_global_settings.iter() {
                match CLAsAvailable::from_str(k) {
                    Ok(agent) => {
                        let tab = v.clone().into_table().unwrap();
                        let mut global_settings = HashMap::new();
                        for (k, v) in tab {
                            global_settings.insert(k, v.into_str().unwrap());
                        }
                        dtncfg.cla_global_settings.insert(agent, global_settings);
                    }
                    Err(message) => {
                        error!("Error parsing cla config: {}", message)
                    }
                }
            }
        }
        if let Ok(services) = s.get_table("services.service") {
            for (_k, v) in services.iter() {
                let tab = v.clone().into_table().unwrap();
                let service_tag: u8 =
                    tab["tag"].clone().into_str().unwrap().parse().expect(
                        "Encountered an error while parsing a service tag from config file",
                    );
                if dtncfg.services.contains_key(&service_tag) {
                    let error = std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!(
                            "Tags must be unique. You tried to use tag {} multiple times.",
                            service_tag
                        ),
                    );
                    panic!("ConfigError: {:?}: {}\n", error.kind(), error);
                }
                let service_payload = tab["payload"].clone().into_str().unwrap();
                debug!("Added custom service: {:?}", service_tag);
                dtncfg.services.insert(service_tag, service_payload);
            }
        }
        if let Ok(discovery_destinations) = s.get_table("discovery_destinations.target") {
            for (_k, v) in discovery_destinations.iter() {
                let tab = v.clone().into_table().unwrap();
                let destination = tab["destination"].clone().into_str().unwrap();
                dtncfg
                    .add_destination(destination.clone())
                    .expect("Encountered an error while parsing discovery address to config");
                debug!("Added discovery address: {:?}", destination);
            }
        }
        dtncfg
            .check_destinations()
            .expect("Encountered an error while checking for the existence of discovery addresses");
        dtncfg
    }
}

impl DtnConfig {
    pub fn new() -> DtnConfig {
        let node_rnd: String = rnd_node_name();
        let local_node_id: EndpointID = format!("dtn://{}", node_rnd).try_into().unwrap();
        DtnConfig {
            debug: false,
            unsafe_httpd: false,
            v4: true,
            v6: false,
            custom_timeout: false,
            enable_period: false,
            nodeid: local_node_id.to_string(),
            host_eid: local_node_id,
            announcement_interval: "2s".parse::<humantime::Duration>().unwrap().into(),
            discovery_destinations: HashMap::new(),
            webport: 3000,
            janitor_interval: "10s".parse::<humantime::Duration>().unwrap().into(),
            endpoints: Vec::new(),
            clas: Vec::new(),
            cla_global_settings: HashMap::new(),
            services: HashMap::new(),
            routing: "epidemic".into(),
            peer_timeout: "20s".parse::<humantime::Duration>().unwrap().into(),
            statics: Vec::new(),
            workdir: std::env::current_dir().unwrap(),
            db: String::from("mem"),
            generate_status_reports: false,
        }
    }
    pub fn set(&mut self, cfg: DtnConfig) {
        self.debug = cfg.debug;
        self.unsafe_httpd = cfg.unsafe_httpd;
        self.v4 = cfg.v4;
        self.v6 = cfg.v6;
        self.custom_timeout = cfg.custom_timeout;
        self.enable_period = cfg.enable_period;
        self.nodeid = cfg.host_eid.to_string();
        self.host_eid = cfg.host_eid;
        self.webport = cfg.webport;
        self.announcement_interval = cfg.announcement_interval;
        self.discovery_destinations = cfg.discovery_destinations;
        self.janitor_interval = cfg.janitor_interval;
        self.endpoints = cfg.endpoints;
        self.clas = cfg.clas;
        self.cla_global_settings = cfg.cla_global_settings;
        self.services = cfg.services;
        self.routing = cfg.routing;
        self.peer_timeout = cfg.peer_timeout;
        self.statics = cfg.statics;
        self.workdir = cfg.workdir;
        self.db = cfg.db;
        self.generate_status_reports = cfg.generate_status_reports;
    }

    /// Helper function that adds discovery destinations to a config struct
    ///
    /// When provided with an IP address without port the default port 3003 is appended
    pub fn add_destination(&mut self, destination: String) -> std::io::Result<()> {
        let addr: SocketAddr = if destination.parse::<SocketAddr>().is_err() {
            let destination = format!("{}:3003", destination);
            destination
                .parse()
                .expect("Error: Unable to parse given IP address into SocketAddr")
        } else {
            destination
                .parse()
                .expect("Error: Unable to parse given IP address into SocketAddr")
        };

        match addr {
            SocketAddr::V4(addr) => {
                if self.v4 {
                    self.discovery_destinations.insert(format!("{}", addr), 0);
                }
            }
            SocketAddr::V6(addr) => {
                if self.v6 {
                    self.discovery_destinations.insert(format!("{}", addr), 0);
                }
            }
        }
        Ok(())
    }

    // If no discovery destination is specified via CLI or config use the default discovery destinations
    // depending on whether to use ipv4 or ipv6
    pub fn check_destinations(&mut self) -> std::io::Result<()> {
        if self.discovery_destinations.is_empty() {
            match (self.v4, self.v6) {
                (true, true) => {
                    self.discovery_destinations
                        .insert("224.0.0.26:3003".to_string(), 0);
                    self.discovery_destinations
                        .insert("[FF02::1]:3003".to_string(), 0);
                }
                (true, false) => {
                    self.discovery_destinations
                        .insert("224.0.0.26:3003".to_string(), 0);
                }
                (false, true) => {
                    self.discovery_destinations
                        .insert("[FF02::1]:3003".to_string(), 0);
                }
                (false, false) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        String::from("Only IP destinations supported at the moment"),
                    ))
                }
            }
        }
        Ok(())
    }

    /// Updates the beacon sequence number everytime a beacon is sent to a specific IP address
    pub fn update_beacon_sequence_number(&mut self, destination: &str) {
        if let Some(sequence) = self.discovery_destinations.get_mut(destination) {
            if *sequence == u32::MAX {
                *sequence = 0;
            } else {
                *sequence += 1;
            }
        }
    }
}
