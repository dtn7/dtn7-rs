use lazy_static::*;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use std::sync::Mutex;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub nodeid: String,
    pub announcement_interval: u64,
    pub janitor_interval: u64,
    pub endpoints: Vec<String>,
}

impl Config {
    pub fn new() -> Config {
        Config {
            nodeid: thread_rng().sample_iter(&Alphanumeric).take(10).collect(),
            announcement_interval: 2000,
            janitor_interval: 10000,
            endpoints: Vec::new(),
        }
    }
    pub fn set(&mut self, cfg: Config) {
        self.nodeid = cfg.nodeid;
        self.announcement_interval = cfg.announcement_interval;
        self.janitor_interval = cfg.janitor_interval;
        self.endpoints = cfg.endpoints;
    }
}

lazy_static! {
    pub static ref CONFIG: Mutex<Config> = Mutex::new(Config::new());
}
