use crate::cla::{ClaSenderTask, ConvergenceLayerAgent};
use crate::{CLAS, CONFIG};
use bp7::EndpointID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum PeerType {
    Static,
    Dynamic,
}

impl std::convert::TryFrom<&str> for PeerType {
    type Error = ();
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match dbg!(value.to_lowercase().as_str()) {
            "static" => Ok(Self::Static),
            "dynamic" => Ok(Self::Dynamic),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum PeerAddress {
    /// Unicast IP node, e.g., reachable via mtcp, tcp or http
    Ip(IpAddr),
    /// Generic peer reachable via a broadcast medium, e.g., LoRa "868_1" "node_id_1"
    BroadcastGeneric(String, String),
    /// A peer reachable via a DNS name and port, e.g., "dtn7.io" 4556
    Dns(String, u16),
    /// Generic peer reachable via a unicast transmission, e.g., MAC address "AA:BB:CC:DD:EE:FF"
    Generic(String),
}

impl From<IpAddr> for PeerAddress {
    fn from(addr: IpAddr) -> Self {
        PeerAddress::Ip(addr)
    }
}
impl From<String> for PeerAddress {
    fn from(addr: String) -> Self {
        let parts = addr.split(':').collect::<Vec<&str>>();
        if parts.len() == 2 {
            let hostname = parts[0].to_string();
            let port = parts[1].parse::<u16>().unwrap();
            PeerAddress::Dns(hostname, port)
        } else {
            PeerAddress::Generic(addr)
        }
    }
}

impl Display for PeerAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerAddress::Ip(addr) => write!(f, "{}", addr),
            PeerAddress::BroadcastGeneric(domain, addr) => write!(f, "{}/{}", domain, addr),
            PeerAddress::Dns(hostname, port) => write!(f, "{}:{}", hostname, port),
            PeerAddress::Generic(addr) => write!(f, "{}", addr),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DtnPeer {
    pub eid: EndpointID,
    pub addr: PeerAddress,
    pub con_type: PeerType,
    pub period: Option<Duration>,
    pub cla_list: Vec<(String, Option<u16>)>,
    pub services: HashMap<u8, String>,
    pub last_contact: u64,
    pub fails: u16,
}

impl DtnPeer {
    pub fn new(
        eid: EndpointID,
        addr: PeerAddress,
        con_type: PeerType,
        period: Option<Duration>,
        cla_list: Vec<(String, Option<u16>)>,
        services: HashMap<u8, String>,
    ) -> DtnPeer {
        DtnPeer {
            eid,
            addr,
            con_type,
            period,
            cla_list,
            services,
            last_contact: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            fails: 0,
        }
    }
    /// Example
    ///
    /// ```
    /// use std::{thread, time};
    /// use dtn7::core::*;
    /// use dtn7::CONFIG;
    ///
    /// let mut peer = helpers::rnd_peer();
    /// let original_time = peer.last_contact;
    /// thread::sleep(time::Duration::from_secs(1));
    /// peer.touch();
    /// assert!(original_time < peer.last_contact);
    /// ```
    pub fn touch(&mut self) {
        self.last_contact = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
    /// Example
    ///
    /// ```
    /// use std::{thread, time::Duration};
    /// use dtn7::core::*;
    /// use dtn7::CONFIG;
    ///
    /// (*CONFIG.lock()).peer_timeout = Duration::from_secs(1);
    /// let mut peer = helpers::rnd_peer();
    /// peer.con_type = PeerType::Dynamic;
    /// assert_eq!(peer.still_valid(), true);
    ///
    /// thread::sleep(Duration::from_secs(2));
    /// assert_eq!(peer.still_valid(), false);
    /// ```

    pub fn still_valid(&self) -> bool {
        if self.con_type == PeerType::Static {
            return true;
        }
        // If a custom peer timeout was specified force remove all peers after specified amount of time
        // Or if no custom peer timeout was specified force remove all peers after default peer timeout
        // that didn't advertise a BeaconPeriod
        let timeout = CONFIG.lock().peer_timeout.as_secs();
        let custom = CONFIG.lock().custom_timeout;
        if (custom && timeout > 0) || self.period.is_none() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time went backwards")
                .as_secs();
            now - self.last_contact < timeout
        // Else if a received beacon contains a BeaconPeriod remove this peer after 2 * received BeaconPeriod
        } else {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();
            now - self.last_contact < self.period.unwrap().as_secs() * 2
        }
    }

    /// Return the peers DTN node name (without URI scheme)
    pub fn node_name(&self) -> String {
        self.eid.node().unwrap_or_default()
    }

    pub fn first_cla(&self) -> Option<ClaSenderTask> {
        for cla in &self.cla_list {
            for cla_instance in &(*CLAS.lock()) {
                if cla.0 == cla_instance.name() && cla_instance.accepting() {
                    let dest = format!(
                        "{}:{}",
                        self.addr,
                        cla.1.unwrap_or_else(|| cla_instance.port())
                    );
                    return Some(ClaSenderTask {
                        tx: cla_instance.channel(),
                        dest,
                        cla_name: cla_instance.name().into(),
                        next_hop: self.eid.clone(),
                    });
                }
            }
        }
        None
    }

    pub fn addr(&self) -> &PeerAddress {
        &self.addr
    }

    pub fn report_fail(&mut self) {
        self.fails += 1;
    }

    pub fn reset_fails(&mut self) {
        self.fails = 0;
    }

    pub fn failed_too_much(&self) -> bool {
        self.fails > 3
    }
}
