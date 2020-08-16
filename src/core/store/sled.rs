use super::BundleStore;
use crate::core::bundlepack::{BundlePack, Constraint};
use crate::CONFIG;
use anyhow::{bail, Result};
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct SledBundleStore {
    bundles: sled::Tree,
    //bundles: HashMap<String, BundlePack>,
}

impl BundleStore for SledBundleStore {
    fn push(&mut self, bp: &BundlePack) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if self.bundles.contains_key(bp.id())? {
            bail!("Bundle already in store!");
        }
        self.bundles.insert(bp.id().to_string(), bp.to_cbor())?;
        self.bundles.flush()?;
        Ok(())
    }
    fn update(&mut self, bp: &BundlePack) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if !self.bundles.contains_key(bp.id())? {
            bail!("Bundle not in store!");
        }
        self.bundles.insert(bp.id().to_string(), bp.to_cbor())?;
        self.bundles.flush()?;
        Ok(())
    }
    fn remove(&mut self, bid: &str) -> Option<BundlePack> {
        let res = self
            .bundles
            .remove(bid)
            .map(|b| b.unwrap().as_ref().into())
            .ok();
        self.bundles.flush();
        res
    }
    fn get(&self, bpid: &str) -> Option<BundlePack> {
        self.bundles
            .get(bpid)
            .map(|b| b.unwrap().as_ref().into())
            .ok()
    }
    fn count(&self) -> u64 {
        self.bundles.len() as u64
    }
    fn all_ids(&self) -> Vec<String> {
        self.bundles
            .iter()
            .keys()
            .filter_map(Result::ok)
            .map(|k| std::str::from_utf8(k.as_ref()).unwrap().into())
            .collect()
    }
    fn has_item(&self, bid: &str) -> bool {
        let res = self.bundles.contains_key(bid);
        if let Ok(contains) = res {
            contains
        } else {
            log::error!("could not query sled database: {:?}", res.err());
            false
        }
    }
    fn pending(&self) -> Vec<String> {
        self.bundles
            .iter()
            .values()
            .filter_map(Result::ok)
            .map(|k| BundlePack::from(k.as_ref()))
            .filter(|e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && (e.has_constraint(Constraint::ForwardPending)
                        || e.has_constraint(Constraint::Contraindicated))
            })
            .map(|k| k.id().into())
            .collect()
    }
    fn ready(&self) -> Vec<String> {
        self.bundles
            .iter()
            .values()
            .filter_map(Result::ok)
            .map(|k| BundlePack::from(k.as_ref()))
            .filter(|e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && !e.has_constraint(Constraint::Contraindicated)
            })
            .map(|k| k.id().into())
            .collect::<Vec<String>>()
    }
    fn forwarding(&self) -> Vec<String> {
        self.bundles
            .iter()
            .values()
            .filter_map(Result::ok)
            .map(|k| BundlePack::from(k.as_ref()))
            .filter(|e| e.has_constraint(Constraint::ForwardPending))
            .map(|k| k.id().into())
            .collect()
    }
    fn bundles(&self) -> Vec<BundlePack> {
        self.bundles
            .iter()
            .values()
            .filter_map(Result::ok)
            .map(|k| BundlePack::from(k.as_ref()))
            .collect()
    }
}

impl SledBundleStore {
    pub fn new() -> SledBundleStore {
        let mut wd = (*CONFIG.lock()).workdir.clone();
        wd.push("store.db");
        let db = sled::open(wd).expect("open sled bundle store");
        SledBundleStore {
            bundles: db.open_tree("bundles").expect("cannot open bundles tree"),
        }
    }
}
