pub mod application_agent;
pub mod bundlepack;
pub mod helpers;
pub mod store;

use crate::cla::ConvergencyLayerAgent;
use crate::core::bundlepack::BundlePack;
use crate::routing::RoutingAgent;
use crate::CONFIG;
use crate::PEERS;
use crate::STATS;
use crate::STORE;
use application_agent::ApplicationAgent;
use bp7::{Bundle, ByteBuffer, EndpointID};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PeerType {
    Static,
    Dynamic,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DtnPeer {
    pub eid: EndpointID,
    pub addr: IpAddr,
    pub con_type: PeerType,
    pub cla_list: Vec<String>,
    pub last_contact: u64,
}

impl DtnPeer {
    pub fn new(
        eid: EndpointID,
        addr: IpAddr,
        con_type: PeerType,
        cla_list: Vec<String>,
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
    /// CONFIG.lock().unwrap().peer_timeout = 1;
    /// let mut peer = helpers::rnd_peer();
    /// assert_eq!(peer.still_valid(), true);
    ///
    /// thread::sleep(time::Duration::from_secs(2));
    /// assert_eq!(peer.still_valid(), false);
    /// ```

    pub fn still_valid(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now - self.last_contact < CONFIG.lock().unwrap().peer_timeout
    }
}
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct DtnStatistics {
    pub incoming: u64,
    pub dups: u64,
    pub outgoing: u64,
    pub delivered: u64,
    pub broken: u64,
}

impl DtnStatistics {
    pub fn new() -> DtnStatistics {
        DtnStatistics {
            incoming: 0,
            dups: 0,
            outgoing: 0,
            delivered: 0,
            broken: 0,
        }
    }
}
#[derive(Debug)]
pub struct DtnCore {
    pub endpoints: Vec<Box<dyn ApplicationAgent + Send>>,
    pub cl_list: Vec<Box<dyn ConvergencyLayerAgent>>,
    pub routing_agent: Box<RoutingAgent>,
}

impl Default for DtnCore {
    fn default() -> Self {
        Self::new()
    }
}

impl DtnCore {
    pub fn new() -> DtnCore {
        DtnCore {
            endpoints: Vec::new(),
            cl_list: Vec::new(),
            //routing_agent: Box::new(crate::routing::flooding::FloodingRoutingAgent::new()),
            routing_agent: Box::new(crate::routing::epidemic::EpidemicRoutingAgent::new()),
        }
    }

    pub fn register_application_agent<T: 'static + ApplicationAgent + Send>(&mut self, aa: T) {
        info!("Registered new application agent for EID: {}", aa.eid());
        self.endpoints.push(Box::new(aa));
    }
    pub fn unregister_application_agent<T: 'static + ApplicationAgent>(&mut self, aa: T) {
        info!("Unregistered application agent for EID: {}", aa.eid());
        self.endpoints
            .iter()
            .position(|n| n.eid() == aa.eid())
            .map(|e| self.endpoints.remove(e));
    }
    pub fn eids(&self) -> Vec<String> {
        self.endpoints.iter().map(|e| e.eid().to_string()).collect()
    }
    pub fn bundles(&self) -> Vec<String> {
        STORE.lock().unwrap().iter().map(|e| e.id()).collect()
    }
    fn is_in_endpoints(&self, eid: &EndpointID) -> bool {
        for aa in self.endpoints.iter() {
            if eid == aa.eid() {
                return true;
            }
        }
        false
    }
    pub fn get_endpoint_mut(
        &mut self,
        eid: &EndpointID,
    ) -> Option<&mut Box<dyn ApplicationAgent + Send>> {
        for aa in self.endpoints.iter_mut() {
            if eid == aa.eid() {
                return Some(aa);
            }
        }
        None
    }
    pub fn get_endpoint(&self, eid: &EndpointID) -> Option<&Box<dyn ApplicationAgent + Send>> {
        for aa in self.endpoints.iter() {
            if eid == aa.eid() {
                return Some(aa);
            }
        }
        None
    }
    pub fn process(&mut self) {
        process_peers();

        // TODO: this all doesn't feel right, not very idiomatic
        let mut del_list: Vec<String> = Vec::new();
        let mut delivery_list: Vec<(EndpointID, Bundle)> = Vec::new();

        for bndl in STORE.lock().unwrap().iter() {
            if self.is_in_endpoints(&bndl.bundle.primary.destination) {
                delivery_list.push((bndl.bundle.primary.destination.clone(), bndl.bundle.clone()));
                STATS.lock().unwrap().delivered += 1;
                break;
            }
        }
        for (eid, bundle) in &delivery_list {
            if let Some(aa) = self.get_endpoint_mut(&eid) {
                info!("Delivering {}", bundle.id());
                del_list.push(bundle.id());
                aa.push(bundle);
            }
        }
        /*self.store
            .iter()
            .position(|x| self.is_in_endpoints(&x.receiver))
            .map(|x| dbg!(x));
        dbg!(&del_list2);*/
        /*self.store
        .iter()
        .find(|x| self.is_in_endpoints(&x.receiver))
        .map(|x| dbg!(x));*/
        /*self.store
        .bundles
        .retain(|&x| self.is_in_endpoints(&x.receiver));*/
        for bp in del_list.iter() {
            STORE.lock().unwrap().remove(bp.to_string());
        }
        let ready: Vec<ByteBuffer> = STORE
            .lock()
            .unwrap()
            .ready()
            .iter()
            .map(|x| x.bundle.clone().to_cbor())
            .collect();
        //self.store.remove_mass(del_list2);
        let keys: Vec<String> = PEERS
            .lock()
            .unwrap()
            .keys()
            .map(|x| x.to_string())
            .collect();
        self.routing_agent.route_all(ready, keys, &self.cl_list);
        /*for cla in &mut self.cl_list {
            cla.scheduled_process(&ready, &keys);
        }*/
    }
    pub fn push(&mut self, bndl: Bundle) {
        STATS.lock().unwrap().incoming += 1;
        let bp = BundlePack::from(bndl);
        if STORE.lock().unwrap().has_item(&bp) {
            debug!("Bundle {} already in store!", bp.id());
            STATS.lock().unwrap().dups += 1;
            return;
        }
        if let Some(aa) = self.get_endpoint_mut(&bp.bundle.primary.destination) {
            if !bp.bundle.primary.has_fragmentation() {
                info!("Delivering {}", bp.id());
                aa.push(&bp.bundle);
                STATS.lock().unwrap().delivered += 1;
                return;
            }
        }
        /*for aa in self.endpoints.iter() {
            if &bndl.primary.destination == aa.eid() && !bndl.primary.has_fragmentation() {
                aa.deliver(&bndl);
                return;
            }
        }*/
        STORE.lock().unwrap().push(bp);
    }
}

/// Removes peers from global peer list that haven't been seen in a while.
pub fn process_peers() {
    PEERS
        .lock()
        .unwrap()
        .retain(|_k, v| v.con_type == PeerType::Static || v.still_valid());
}
