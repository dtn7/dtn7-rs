use super::bundlepack::{BundlePack, Constraint};
use std::collections::HashMap;
use std::fmt::Debug;
use std::slice::{Iter, IterMut};

pub trait BundleStore: Debug {
    fn push(&mut self, bp: &BundlePack);
    fn remove(&mut self, bid: String) -> Option<BundlePack>;
    /*   fn remove_mass(&mut self, idxs: Vec<usize>);
    fn iter(&self) -> Iter<BundlePack>;
    fn iter_mut(&mut self) -> IterMut<BundlePack>;*/
    fn count(&self) -> u64;
    fn all_ids(&self) -> Vec<String>;
    fn has_item(&self, bp: &BundlePack) -> bool;
    fn pending(&self) -> Vec<&BundlePack>;
    fn ready(&self) -> Vec<&BundlePack>;
    fn forwarding(&self) -> Vec<&BundlePack>;
    fn bundles(&mut self) -> Vec<&BundlePack>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleBundleStore {
    bundles: HashMap<String, BundlePack>,
}

impl BundleStore for SimpleBundleStore {
    fn push(&mut self, bp: &BundlePack) {
        // TODO: check for duplicates, update, remove etc
        //self.bundles.push(bp.clone());
        let entry = self.bundles.entry(bp.id()).or_insert_with(|| bp.clone());
        *entry = bp.clone();
    }
    fn remove(&mut self, bid: String) -> Option<BundlePack> {
        self.bundles.remove(&bid)
        /*self.iter()
        .position(|n| n.id() == bid)
        .map(|e| self.bundles.remove(e))*/
        // TODO: once feature leaves unstable switch code
        // self.bundles.remove_item(bp);
    }
    /*fn remove_mass(&mut self, idxs: Vec<usize>) {
        for idx in idxs.iter() {
            self.bundles.remove(*idx);
        }
    }*/
    /*fn iter(&self) -> Iter<BundlePack> {
        self.bundles.iter()
    }
    fn iter_mut(&mut self) -> IterMut<BundlePack> {
        self.bundles.iter_mut()
    }*/
    fn count(&self) -> u64 {
        self.bundles.len() as u64
    }
    fn all_ids(&self) -> Vec<String> {
        self.bundles.keys().map(|i| i.clone()).collect()
    }
    fn has_item(&self, bp: &BundlePack) -> bool {
        self.bundles.contains_key(&bp.id())
        /*for item in &self.bundles {
            if bp.id() == item.id() {
                return true;
            }
        }
        false*/
    }
    fn pending(&self) -> Vec<&BundlePack> {
        self.bundles
            .values()
            .filter(|&e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && e.has_constraint(Constraint::Contraindicated)
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
    fn bundles(&mut self) -> Vec<&BundlePack> {
        self.bundles.values().collect()
    }
}

impl Default for SimpleBundleStore {
    fn default() -> Self {
        SimpleBundleStore::new()
    }
}
impl SimpleBundleStore {
    pub fn new() -> SimpleBundleStore {
        SimpleBundleStore {
            bundles: HashMap::new(),
        }
    }
}
