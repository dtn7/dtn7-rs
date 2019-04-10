pub mod application_agent;
pub mod bundlepack;
pub mod helpers;
pub mod store;

use crate::cla::ConvergencyLayerAgent;
use crate::core::bundlepack::BundlePack;
use crate::dtnconfig;
use crate::routing::RoutingAgent;
use application_agent::ApplicationAgent;
use bp7::{Bundle, ByteBuffer, EndpointID};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use store::{BundleStore, SimpleBundleStore};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PeerType {
    Static,
    Dynamic,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DtnPeer {
    pub eid: EndpointID,
    pub addr: IpAddr,
    pub con_type: PeerType,
    pub cl_list: Vec<String>,
    pub last_contact: u64,
}

impl DtnPeer {
    pub fn new(eid: EndpointID, addr: IpAddr, con_type: PeerType, cl_list: Vec<String>) -> DtnPeer {
        DtnPeer {
            eid,
            addr,
            con_type,
            cl_list,
            last_contact: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }
    pub fn touch(&mut self) {
        self.last_contact = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct DtnStatistics {
    pub incoming: u64,
    pub dups: u64,
    pub outgoing: u64,
    pub delivered: u64,
    pub broken: u64,
}

#[derive(Debug)]
pub struct DtnCore {
    pub nodeid: String,
    pub endpoints: Vec<Box<dyn ApplicationAgent + Send>>,
    pub store: Box<dyn BundleStore + Send>,
    pub stats: DtnStatistics,
    pub peers: HashMap<IpAddr, DtnPeer>,
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
            nodeid: dtnconfig::CONFIG.lock().unwrap().nodeid.clone(),
            endpoints: Vec::new(),
            store: Box::new(SimpleBundleStore::new()),
            stats: DtnStatistics {
                incoming: 0,
                dups: 0,
                outgoing: 0,
                delivered: 0,
                broken: 0,
            },
            peers: HashMap::new(),
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
        self.store.iter().map(|e| e.id()).collect()
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
        // TODO: this all doesn't feel right, not very idiomatic
        let mut del_list: Vec<String> = Vec::new();
        let mut delivery_list: Vec<(EndpointID, Bundle)> = Vec::new();

        for bndl in self.store.iter() {
            if self.is_in_endpoints(&bndl.bundle.primary.destination) {
                delivery_list.push((bndl.bundle.primary.destination.clone(), bndl.bundle.clone()));
                self.stats.delivered += 1;
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
            self.store.remove(bp.to_string());
        }
        let ready: Vec<ByteBuffer> = self
            .store
            .ready()
            .iter()
            .map(|x| x.bundle.clone().to_cbor())
            .collect();
        //self.store.remove_mass(del_list2);
        let keys: Vec<String> = self.peers.keys().map(|x| x.to_string()).collect();
        self.routing_agent.route_all(ready, keys, &self.cl_list);
        /*for cla in &mut self.cl_list {
            cla.scheduled_process(&ready, &keys);
        }*/
    }
    pub fn push(&mut self, bndl: Bundle) {
        self.stats.incoming += 1;
        let bp = BundlePack::from(bndl);
        if self.store.has_item(&bp) {
            debug!("Bundle {} already in store!", bp.id());
            self.stats.dups += 1;
            return;
        }
        if let Some(aa) = self.get_endpoint_mut(&bp.bundle.primary.destination) {
            if !bp.bundle.primary.has_fragmentation() {
                info!("Delivering {}", bp.id());
                aa.push(&bp.bundle);
                self.stats.delivered += 1;
                return;
            }
        }
        /*for aa in self.endpoints.iter() {
            if &bndl.primary.destination == aa.eid() && !bndl.primary.has_fragmentation() {
                aa.deliver(&bndl);
                return;
            }
        }*/
        self.store.push(bp);
    }
}
