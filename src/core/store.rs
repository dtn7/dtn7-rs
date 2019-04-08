use super::bundlepack::{BundlePack, Constraint};
use std::fmt::Debug;
use std::slice::{Iter, IterMut};

pub trait BundleStore: Debug {
    fn push(&mut self, bp: BundlePack);
    fn remove(&mut self, bid: String) -> Option<BundlePack>;
    fn remove_mass(&mut self, idxs: Vec<usize>);
    fn iter(&self) -> Iter<BundlePack>;
    fn iter_mut(&mut self) -> IterMut<BundlePack>;
    fn count(&self) -> u64;
    fn all(&self) -> &[BundlePack];
    fn has_item(&self, bp: &BundlePack) -> bool;
    fn pending(&self) -> Vec<&BundlePack>;
    fn ready(&self) -> Vec<&BundlePack>;
    fn forwarding(&self) -> Vec<&BundlePack>;
    fn bundles(&mut self) -> &Vec<BundlePack>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimpleBundleStore {
    bundles: Vec<BundlePack>,
}

impl BundleStore for SimpleBundleStore {
    fn push(&mut self, bp: BundlePack) {
        self.bundles.push(bp);
    }
    fn remove(&mut self, bid: String) -> Option<BundlePack> {
        self.iter()
            .position(|n| n.id() == bid)
            .map(|e| self.bundles.remove(e))
        // TODO: once feature leaves unstable switch code
        // self.bundles.remove_item(bp);
    }
    fn remove_mass(&mut self, idxs: Vec<usize>) {
        for idx in idxs.iter() {
            self.bundles.remove(*idx);
        }
    }
    fn iter(&self) -> Iter<BundlePack> {
        self.bundles.iter()
    }
    fn iter_mut(&mut self) -> IterMut<BundlePack> {
        self.bundles.iter_mut()
    }
    fn count(&self) -> u64 {
        self.bundles.len() as u64
    }
    fn all(&self) -> &[BundlePack] {
        &self.bundles
    }
    fn has_item(&self, bp: &BundlePack) -> bool {
        for item in &self.bundles {
            if bp.id() == item.id() {
                return true;
            }
        }
        false
    }
    fn pending(&self) -> Vec<&BundlePack> {
        self.bundles
            .iter()
            .filter(|&e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && e.has_constraint(Constraint::Contraindicated)
            })
            .collect::<Vec<&BundlePack>>()
    }
    fn ready(&self) -> Vec<&BundlePack> {
        self.bundles
            .iter()
            .filter(|&e| {
                !e.has_constraint(Constraint::ReassemblyPending)
                    && !e.has_constraint(Constraint::Contraindicated)
            })
            .collect::<Vec<&BundlePack>>()
    }
    fn forwarding(&self) -> Vec<&BundlePack> {
        self.bundles
            .iter()
            .filter(|&e| e.has_constraint(Constraint::ForwardPending))
            .collect::<Vec<&BundlePack>>()
    }
    fn bundles(&mut self) -> &Vec<BundlePack> {
        &self.bundles
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
            bundles: Vec::new(),
        }
    }
}
