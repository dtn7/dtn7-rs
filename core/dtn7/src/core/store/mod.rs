use crate::core::bundlepack::BundlePack;
use crate::core::bundlepack::Constraint;
use anyhow::Result;
use bp7::Bundle;
use enum_dispatch::enum_dispatch;
use std::collections::HashSet;
use std::fmt::Debug;

mod mem;
pub use mem::InMemoryBundleStore;

#[cfg(feature = "store_sled")]
mod sled;

#[cfg(feature = "store_sneakers")]
mod sneakers;

#[cfg(feature = "store_sled")]
pub use self::sled::SledBundleStore;

#[cfg(feature = "store_sneakers")]
pub use sneakers::SneakersBundleStore;

#[enum_dispatch]
#[derive(Debug)]
pub enum BundleStoresEnum {
    #[cfg(feature = "store_sled")]
    SledBundleStore,
    InMemoryBundleStore,
    #[cfg(feature = "store_sneakers")]
    SneakersBundleStore,
}

#[enum_dispatch(BundleStoresEnum)]
pub trait BundleStore: Debug {
    fn push(&mut self, bp: &Bundle) -> Result<()>;
    fn update_metadata(&mut self, bp: &BundlePack) -> Result<()>;
    fn remove(&mut self, bid: &str) -> Result<()>;
    fn count(&self) -> u64;
    fn all_ids(&self) -> Vec<String>;
    fn has_item(&self, bid: &str) -> bool;
    fn pending(&self) -> Vec<String>;
    fn forwarding(&self) -> Vec<String> {
        let criteria: HashSet<Constraint> = vec![Constraint::ForwardPending].into_iter().collect();
        self.filter(&criteria)
    }
    fn filter(&self, criteria: &HashSet<Constraint>) -> Vec<String>;
    fn bundles(&self) -> Vec<BundlePack>;
    fn bundles_status(&self) -> Vec<String> {
        self.bundles().iter().map(|bp| bp.to_string()).collect()
    }
    fn src_dst_ts(&self) -> Vec<String> {
        self.bundles()
            .iter()
            .map(|bp| {
                format!(
                    "{} {} {} {}",
                    bp.source, bp.destination, bp.creation_time, bp.size
                )
            })
            .collect()
    }
    fn filter_addr(&self, criteria: &str) -> Vec<String> {
        self.bundles()
            .iter()
            .filter(|bp| !bp.has_constraint(Constraint::Deleted))
            .filter(|bp| {
                bp.source.to_string().contains(criteria)
                    || bp.destination.to_string().contains(criteria)
            })
            .map(|bp| bp.id.clone())
            .collect()
    }
    fn get_bundle(&self, bpid: &str) -> Option<Bundle>;
    fn get_metadata(&self, bpid: &str) -> Option<BundlePack>;
}

pub fn bundle_stores() -> Vec<&'static str> {
    vec![
        "mem",
        #[cfg(feature = "store_sled")]
        "sled",
        #[cfg(feature = "store_sneakers")]
        "sneakers",
    ]
}

pub fn new(bundlestore: &str) -> BundleStoresEnum {
    match bundlestore {
        "mem" => mem::InMemoryBundleStore::new().into(),
        #[cfg(feature = "store_sled")]
        "sled" => sled::SledBundleStore::new().into(),
        #[cfg(feature = "store_sneakers")]
        "sneakers" => sneakers::SneakersBundleStore::new().into(),
        _ => panic!("Unknown bundle store {}", bundlestore),
    }
}
