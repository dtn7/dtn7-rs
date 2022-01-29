use super::BundleStore;
use crate::core::bundlepack::{BundlePack, Constraint};
use anyhow::{bail, Result};
use bp7::Bundle;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InMemoryBundleStore {
    bundles: HashMap<String, Bundle>,
    metadata: HashMap<String, BundlePack>,
}

impl BundleStore for InMemoryBundleStore {
    fn push(&mut self, bndl: &Bundle) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        let bp = BundlePack::from(bndl);
        if self.bundles.contains_key(bp.id()) {
            debug!("Bundle {} already in store, updating it!", bndl.id());
        } else {
            self.metadata.insert(bp.id().to_string(), bp);
        }
        let bid = bndl.id();
        debug!("inserting bundle {} in to store", bid);
        let b = bndl.clone();
        let _ret = self.bundles.insert(bid, b);

        Ok(())
    }
    fn update_metadata(&mut self, bp: &BundlePack) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if !self.metadata.contains_key(bp.id()) {
            bail!("Bundle not in store!");
        }
        self.metadata.insert(bp.id().to_string(), bp.clone());
        Ok(())
    }
    fn remove(&mut self, bid: &str) -> Result<()> {
        if let Some(mut meta) = self.get_metadata(bid) {
            meta.clear_constraints();
            meta.add_constraint(Constraint::Deleted);
            self.update_metadata(&meta)?;
        }
        if self.bundles.remove(bid).is_none() {
            bail!("Bundle not in store!");
        }
        Ok(())
    }

    fn count(&self) -> u64 {
        self.bundles.len() as u64
    }
    fn all_ids(&self) -> Vec<String> {
        self.bundles.keys().cloned().collect()
    }
    fn has_item(&self, bid: &str) -> bool {
        self.bundles.contains_key(bid)
    }
    fn filter(&self, criteria: &HashSet<Constraint>) -> Vec<String> {
        self.metadata
            .values()
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
            .values()
            .filter(|&e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && (e.has_constraint(Constraint::ForwardPending)
                        || e.has_constraint(Constraint::Contraindicated))
            })
            .map(|b| b.id().into())
            .collect()
    }
    fn bundles(&self) -> Vec<BundlePack> {
        self.metadata.values().cloned().collect::<Vec<BundlePack>>()
    }

    fn get_bundle(&self, bpid: &str) -> Option<Bundle> {
        debug!("get_bundle {}", bpid);
        self.bundles.get(bpid).cloned()
    }

    fn get_metadata(&self, bpid: &str) -> Option<BundlePack> {
        self.metadata.get(bpid).cloned()
    }
}

impl InMemoryBundleStore {
    pub fn new() -> InMemoryBundleStore {
        InMemoryBundleStore {
            bundles: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}
