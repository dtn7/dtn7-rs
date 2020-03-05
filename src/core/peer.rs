use crate::CONFIG;
use bp7::EndpointID;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum PeerType {
    Static,
    Dynamic,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DtnPeer {
    pub eid: EndpointID,
    pub addr: IpAddr,
    pub con_type: PeerType,
    pub cla_list: Vec<(String, Option<u16>)>,
    pub last_contact: u64,
}

impl DtnPeer {
    pub fn new(
        eid: EndpointID,
        addr: IpAddr,
        con_type: PeerType,
        cla_list: Vec<(String, Option<u16>)>,
    ) -> DtnPeer {
        DtnPeer {
            eid,
            addr,
            con_type,
            cla_list,
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
    /// use std::{thread, time};
    /// use dtn7::core::*;
    /// use dtn7::CONFIG;
    ///
    /// (*CONFIG.lock()).peer_timeout = 1;
    /// let mut peer = helpers::rnd_peer();
    /// assert_eq!(peer.still_valid(), true);
    ///
    /// thread::sleep(time::Duration::from_secs(2));
    /// assert_eq!(peer.still_valid(), false);
    /// ```

    pub fn still_valid(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_secs();
        now - self.last_contact < (*CONFIG.lock()).peer_timeout.as_secs()
    }

    pub fn node_name(&self) -> String {
        self.eid.node_part().unwrap_or_default()
    }
    pub fn first_cla(&self) -> Option<crate::cla::ClaSender> {
        for c in self.cla_list.iter() {
            if crate::cla::convergency_layer_agents().contains(&c.0.as_str()) {
                let sender = crate::cla::ClaSender {
                    remote: self.addr,
                    port: c.1,
                    agent: c.0.clone(),
                };
                return Some(sender);
            }
        }
        None
    }
    pub fn addr(&self) -> &IpAddr {
        &self.addr
    }
}
