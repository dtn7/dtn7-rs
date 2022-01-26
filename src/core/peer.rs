use crate::cla::ConvergenceLayerAgents;
use crate::CONFIG;
use bp7::EndpointID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum PeerType {
    Static,
    Dynamic,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub enum PeerAddress {
    Ip(IpAddr),
    Generic(String),
}

impl From<IpAddr> for PeerAddress {
    fn from(addr: IpAddr) -> Self {
        PeerAddress::Ip(addr)
    }
}
impl From<String> for PeerAddress {
    fn from(addr: String) -> Self {
        PeerAddress::Generic(addr)
    }
}

impl Display for PeerAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerAddress::Ip(addr) => write!(f, "{}", addr),
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
    pub cla_list: Vec<(ConvergenceLayerAgents, Option<u16>)>,
    pub services: HashMap<u8, String>,
    pub last_contact: u64,
}

impl DtnPeer {
    pub fn new(
        eid: EndpointID,
        addr: PeerAddress,
        con_type: PeerType,
        period: Option<Duration>,
        cla_list: Vec<(ConvergenceLayerAgents, Option<u16>)>,
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
        let timeout = (*CONFIG.lock()).peer_timeout.as_secs();
        let custom = (*CONFIG.lock()).custom_timeout;
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

    pub fn node_name(&self) -> String {
        self.eid.node().unwrap_or_default()
    }
    pub fn first_cla(&self) -> Option<crate::cla::ClaSender> {
        if let Some((c, port)) = self.cla_list.first() {
            let sender = crate::cla::ClaSender {
                remote: self.addr.clone(),
                port: *port,
                agent: *c,
            };
            return Some(sender);
        }
        None
    }
    pub fn addr(&self) -> &PeerAddress {
        &self.addr
    }
}
