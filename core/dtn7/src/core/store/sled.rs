use super::BundleStore;
use crate::core::bundlepack::{BundlePack, Constraint};
use crate::CONFIG;
use anyhow::{bail, Result};
use bp7::Bundle;
use log::{debug, error};
use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct SledBundleStore {
    bundles: sled::Tree,
    metadata: sled::Tree,
    //bundles: HashMap<String, BundlePack>,
}

impl BundleStore for SledBundleStore {
    fn push(&mut self, bndl: &Bundle) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if self.bundles.contains_key(bndl.id())? {
            debug!("Bundle {} already in store, updating it!", bndl.id());
        } else {
            let bp = BundlePack::from(bndl);
            if self.metadata.contains_key(bp.id())? {
                bail!("Bundle metadata already in store!");
            }
            self.metadata.insert(bp.id().to_string(), bp.to_cbor())?;
            self.metadata.flush()?;
        }
        // TODO: eliminate double clone (here and in BundlePack::from)
        self.bundles.insert(bndl.id(), bndl.clone().to_cbor())?;
        self.bundles.flush()?;

        Ok(())
    }
    fn update_metadata(&mut self, bp: &BundlePack) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if !self.metadata.contains_key(bp.id())? {
            bail!("Bundle not in store!");
        }
        self.metadata.insert(bp.id().to_string(), bp.to_cbor())?;
        self.metadata.flush()?;
        Ok(())
    }
    fn remove(&mut self, bid: &str) -> Result<()> {
        if let Some(mut meta) = self.get_metadata(bid) {
            meta.clear_constraints();
            meta.add_constraint(Constraint::Deleted);
            self.update_metadata(&meta)?;
        }
        let _res = self.bundles.remove(bid)?;
        if let Err(err) = self.bundles.flush() {
            error!("Could not flush database: {}", err);
        }

        Ok(())
    }
    /*fn get(&self, bpid: &str) -> Option<BundlePack> {
        self.bundles
            .get(bpid)
            .map(|b| b.unwrap().as_ref().into())
            .ok()
    }*/
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
            error!("could not query sled database: {:?}", res.err());
            false
        }
    }
    fn filter(&self, criteria: &HashSet<Constraint>) -> Vec<String> {
        self.metadata
            .iter()
            .values()
            .filter_map(Result::ok)
            .map(|k| BundlePack::from(k.as_ref()))
            .filter(|e| {
                for c in criteria {
                    if !e.has_constraint(*c) {
                        return false;
                    }
                }
                true
            })
            .map(|k| k.id().into())
            .collect()
    }

    fn pending(&self) -> Vec<String> {
        self.metadata
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

    fn bundles(&self) -> Vec<BundlePack> {
        self.metadata
            .iter()
            .values()
            .filter_map(Result::ok)
            .map(|k| BundlePack::from(k.as_ref()))
            .collect()
    }

    fn get_bundle(&self, bpid: &str) -> Option<bp7::Bundle> {
        self.bundles
            .get(bpid)
            .map(|b| Bundle::try_from(b.unwrap().as_ref().to_vec()).unwrap())
            .ok()
    }

    fn get_metadata(&self, bpid: &str) -> Option<BundlePack> {
        self.metadata
            .get(bpid)
            .map(|b| b.unwrap().as_ref().into())
            .ok()
    }
}

impl SledBundleStore {
    pub fn new() -> SledBundleStore {
        let mut wd = (*CONFIG.lock()).workdir.clone();
        wd.push("store.db");
        let db = sled::open(wd).expect("open sled bundle store");
        SledBundleStore {
            bundles: db.open_tree("bundles").expect("cannot open bundles tree"),
            metadata: db.open_tree("metadata").expect("cannot open bundles tree"),
        }
    }
}

impl Default for SledBundleStore {
    fn default() -> Self {
        Self::new()
    }
}
