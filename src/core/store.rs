use super::bundlepack::{BundlePack, Constraint};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::fmt::Debug;

pub trait BundleStore: Debug {
    fn push(&mut self, bp: &BundlePack) -> Result<()>;
    fn update(&mut self, bp: &BundlePack) -> Result<()>;
    fn remove(&mut self, bid: &str) -> Option<BundlePack>;
    fn count(&self) -> u64;
    fn all_ids(&self) -> Vec<String>;
    fn has_item(&self, bid: &str) -> bool;
    fn pending(&self) -> Vec<&BundlePack>;
    fn ready(&self) -> Vec<&BundlePack>;
    fn forwarding(&self) -> Vec<&BundlePack>;
    fn bundles(&self) -> Vec<&BundlePack>;
    fn bundles_status(&self) -> Vec<String> {
        self.bundles().iter().map(|bp| bp.to_string()).collect()
    }
    fn get(&self, bpid: &str) -> Option<&BundlePack>;
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct InMemoryBundleStore {
    bundles: HashMap<String, BundlePack>,
}

impl BundleStore for InMemoryBundleStore {
    fn push(&mut self, bp: &BundlePack) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if self.bundles.contains_key(bp.id()) {
            bail!("Bundle already in store!");
        }
        self.bundles.insert(bp.id().to_string(), bp.clone());
        Ok(())
    }
    fn update(&mut self, bp: &BundlePack) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if !self.bundles.contains_key(bp.id()) {
            bail!("Bundle not in store!");
        }
        self.bundles.insert(bp.id().to_string(), bp.clone());
        Ok(())
    }
    fn remove(&mut self, bid: &str) -> Option<BundlePack> {
        self.bundles.remove(bid)
    }
    fn get(&self, bpid: &str) -> Option<&BundlePack> {
        self.bundles.get(bpid)
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
    fn pending(&self) -> Vec<&BundlePack> {
        self.bundles
            .values()
            .filter(|&e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && (e.has_constraint(Constraint::ForwardPending)
                        || e.has_constraint(Constraint::Contraindicated))
            })
            .collect::<Vec<&BundlePack>>()
    }
    fn ready(&self) -> Vec<&BundlePack> {
        self.bundles
            .values()
            .filter(|&e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && !e.has_constraint(Constraint::Contraindicated)
            })
            .collect::<Vec<&BundlePack>>()
    }
    fn forwarding(&self) -> Vec<&BundlePack> {
        self.bundles
            .values()
            .filter(|&e| e.has_constraint(Constraint::ForwardPending))
            .collect::<Vec<&BundlePack>>()
    }
    fn bundles(&self) -> Vec<&BundlePack> {
        self.bundles.values().collect::<Vec<&BundlePack>>()
    }
}

impl InMemoryBundleStore {
    pub fn new() -> InMemoryBundleStore {
        InMemoryBundleStore {
            bundles: HashMap::new(),
        }
    }
}
