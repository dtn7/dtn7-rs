use lazy_static::*;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct DtnConfig {
    pub nodeid: String,
    pub announcement_interval: u64,
    pub janitor_interval: u64,
    pub endpoints: Vec<String>,
    pub clas: Vec<String>,
    pub routing: String,
}

impl DtnConfig {
    pub fn new() -> DtnConfig {
        let node_rnd: String = thread_rng().sample_iter(&Alphanumeric).take(10).collect();
        DtnConfig {
            nodeid: format!("dtn://{}", node_rnd),
            announcement_interval: 2000,
            janitor_interval: 10000,
            endpoints: Vec::new(),
            clas: Vec::new(),
            routing: "epidemic".into(),
        }
    }
    pub fn set(&mut self, cfg: DtnConfig) {
        self.nodeid = cfg.nodeid;
        self.announcement_interval = cfg.announcement_interval;
        self.janitor_interval = cfg.janitor_interval;
        self.endpoints = cfg.endpoints;
        self.clas = cfg.clas;
        self.routing = cfg.routing;
    }
}
