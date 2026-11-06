use crate::cla::{ClaSenderTask, ConvergenceLayerAgent};
use crate::{CLAS, CONFIG};
use bp7::EndpointID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Display;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum PeerType {
    Static,
    Dynamic,
}

impl std::convert::TryFrom<&str> for PeerType {
    type Error = ();
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
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
        PeerAddress::Generic(addr)
    }
}

impl PeerAddress {
    /// Returns the address combined with the given port, e.g., `10.0.0.1:4556`,
    /// `[fe80::1]:4556` or `node1.example.org:4556`.
    pub fn with_port(&self, port: u16) -> String {
        match self {
            PeerAddress::Ip(ip) => SocketAddr::new(*ip, port).to_string(),
            _ => format!("{}:{}", self, port),
        }
    }
}

impl Display for PeerAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerAddress::Ip(addr) => write!(f, "{}", addr),
            PeerAddress::BroadcastGeneric(domain, addr) => write!(f, "{}/{}", domain, addr),
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
        let (timeout, custom_timeout) = {
            let config = CONFIG.lock();
            (config.peer_timeout.as_secs(), config.custom_timeout)
        };
        // A custom peer timeout applies to all peers. Otherwise, peers advertising a
        // beacon period are removed after twice that period, all others after the
        // default peer timeout.
        let max_silence = match self.period {
            Some(period) if !(custom_timeout && timeout > 0) => period.as_secs() * 2,
            _ => timeout,
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        // the clock may have been set back since the last contact
        now.saturating_sub(self.last_contact) < max_silence
    }

    /// Return the peers DTN node name (without URI scheme)
    pub fn node_name(&self) -> String {
        self.eid.node().unwrap_or_default()
    }

    pub fn first_cla(&self) -> Option<ClaSenderTask> {
        for cla in &self.cla_list {
            for cla_instance in &(*CLAS.lock()) {
                if cla.0 == cla_instance.name() && cla_instance.accepting() {
                    let dest = self
                        .addr
                        .with_port(cla.1.unwrap_or_else(|| cla_instance.port()));
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

    /// Updates a known peer with the information of a newly discovered or added one.
    ///
    /// A static peer stays static and its failed transmissions are kept, so
    /// discovery beacons neither turn it into a dynamic peer nor reset its failures.
    pub fn update(&mut self, peer: DtnPeer) {
        if self.con_type != PeerType::Static {
            self.con_type = peer.con_type;
        }
        self.addr = peer.addr;
        self.period = peer.period;
        self.cla_list = peer.cla_list;
        self.services = peer.services;
        self.last_contact = peer.last_contact;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryFrom;

    fn peer(con_type: PeerType, addr: PeerAddress) -> DtnPeer {
        DtnPeer::new(
            EndpointID::try_from("dtn://peer-test/".to_string()).unwrap(),
            addr,
            con_type,
            None,
            vec![("tcp".into(), Some(4556))],
            HashMap::new(),
        )
    }

    #[test]
    fn address_with_port() {
        let v4 = PeerAddress::Ip("10.0.0.1".parse().unwrap());
        let v6 = PeerAddress::Ip("fe80::1".parse().unwrap());
        let host = PeerAddress::Generic("node1.example.org".into());
        assert_eq!(v4.with_port(4556), "10.0.0.1:4556");
        assert_eq!(v6.with_port(4556), "[fe80::1]:4556");
        assert_eq!(host.with_port(4556), "node1.example.org:4556");
    }

    #[test]
    fn update_keeps_static_peers_static() {
        let mut known = peer(
            PeerType::Static,
            PeerAddress::Ip("10.0.0.1".parse().unwrap()),
        );
        known.report_fail();
        let mut discovered = peer(
            PeerType::Dynamic,
            PeerAddress::Ip("10.0.0.2".parse().unwrap()),
        );
        discovered.period = Some(Duration::from_secs(2));

        known.update(discovered);
        assert_eq!(known.con_type, PeerType::Static);
        assert_eq!(known.fails, 1);
        assert_eq!(known.addr, PeerAddress::Ip("10.0.0.2".parse().unwrap()));
        assert_eq!(known.period, Some(Duration::from_secs(2)));
    }

    #[test]
    fn update_can_make_dynamic_peers_static() {
        let mut known = peer(
            PeerType::Dynamic,
            PeerAddress::Ip("10.0.0.1".parse().unwrap()),
        );
        known.update(peer(
            PeerType::Static,
            PeerAddress::Ip("10.0.0.1".parse().unwrap()),
        ));
        assert_eq!(known.con_type, PeerType::Static);
    }

    #[test]
    fn last_contact_in_the_future_does_not_panic() {
        let mut dynamic = peer(
            PeerType::Dynamic,
            PeerAddress::Ip("10.0.0.1".parse().unwrap()),
        );
        // e.g., after the clock was set back
        dynamic.last_contact += 3600;
        assert!(dynamic.still_valid());
    }
}
