use crate::core::bundlepack::BundlePack;
use anyhow::Result;
use std::fmt::Debug;

mod mem;
mod sled;

pub use self::sled::SledBundleStore;
pub use mem::InMemoryBundleStore;

pub trait BundleStore: Debug + Send {
    fn push(&mut self, bp: &BundlePack) -> Result<()>;
    fn update(&mut self, bp: &BundlePack) -> Result<()>;
    fn remove(&mut self, bid: &str) -> Option<BundlePack>;
    fn count(&self) -> u64;
    fn all_ids(&self) -> Vec<String>;
    fn has_item(&self, bid: &str) -> bool;
    fn pending(&self) -> Vec<String>;
    fn ready(&self) -> Vec<String>;
    fn forwarding(&self) -> Vec<String>;
    fn bundles(&self) -> Vec<BundlePack>;
    fn bundles_status(&self) -> Vec<String> {
        self.bundles().iter().map(|bp| bp.to_string()).collect()
    }
    fn get(&self, bpid: &str) -> Option<BundlePack>;
}

pub fn bundle_stores() -> Vec<&'static str> {
    vec!["mem", "sled"]
}

pub fn new(bundlestore: &str) -> Box<dyn BundleStore + Send> {
    match bundlestore {
        "mem" => Box::new(mem::InMemoryBundleStore::new()),
        "sled" => Box::new(sled::SledBundleStore::new()),
        _ => panic!("Unknown bundle store {}", bundlestore),
    }
}
