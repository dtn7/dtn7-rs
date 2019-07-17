pub mod core;

pub mod dtnd;

pub mod cla;

pub mod dtnconfig;

pub mod routing;

use crate::core::store::{BundleStore, SimpleBundleStore};
use crate::core::DtnStatistics;
pub use dtnconfig::DtnConfig;

pub use crate::core::{DtnCore, DtnPeer};

use lazy_static::*;
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    pub static ref CONFIG: Mutex<DtnConfig> = Mutex::new(DtnConfig::new());
    pub static ref DTNCORE: Mutex<DtnCore> = Mutex::new(DtnCore::new());
    pub static ref PEERS: Mutex<HashMap<String, DtnPeer>> = Mutex::new(HashMap::new());
    pub static ref STATS: Mutex<DtnStatistics> = Mutex::new(DtnStatistics::new());
    pub static ref STORE: Mutex<Box<dyn BundleStore + Send>> =
        Mutex::new(Box::new(SimpleBundleStore::new()));
}
