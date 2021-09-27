use super::BundleStore;
use crate::core::bundlepack::{self, BundlePack, Constraint};
use crate::CONFIG;
use anyhow::{bail, Result};
use bp7::{Bundle, EndpointID};
use d7sneakers::{Constraints, SneakerWorld};
use log::debug;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct SneakersBundleStore {
    store: SneakerWorld,
}

impl SneakersBundleStore {
    fn eid_from_strings(
        &self,
        name: Option<String>,
        service: Option<String>,
    ) -> Result<EndpointID> {
        let eid_string = if name.clone().unwrap_or_default().parse::<u64>().is_ok()
            && (service.is_none() || service.as_ref().unwrap().parse::<u64>().is_ok())
        {
            format!(
                "ipn:{}.{}",
                name.unwrap_or_default(),
                service.unwrap_or_default()
            )
        } else {
            format!(
                "dtn:{}.{}",
                name.unwrap_or_default(),
                service.unwrap_or_default()
            )
        };
        Ok(EndpointID::try_from(eid_string)?)
    }
}
impl BundleStore for SneakersBundleStore {
    fn push(&mut self, bndl: &Bundle) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if self.store.db.exists(&bndl.id()) {
            debug!("Bundle {} already in store, updating it!", bndl.id());
        }
        self.store.push(&mut bndl.clone())?;
        Ok(())
    }
    fn update_metadata(&mut self, bp: &BundlePack) -> Result<()> {
        // TODO: check for duplicates, update, remove etc
        if !self.store.db.exists(bp.id()) {
            bail!("Bundle not in store!");
        }
        let c = convert_hashset_to_constraints(&bp.constraints);
        self.store.db.set_constraints(bp.id(), c)?;
        Ok(())
    }
    fn remove(&mut self, bid: &str) -> Result<()> {
        if let Some(mut meta) = self.get_metadata(bid) {
            meta.clear_constraints();
            meta.add_constraint(Constraint::Deleted);
            self.update_metadata(&meta)?;
        }
        self.store.remove(bid)
    }

    fn count(&self) -> u64 {
        self.store.db.len() as u64
    }
    fn all_ids(&self) -> Vec<String> {
        self.store.db.ids()
    }
    fn has_item(&self, bid: &str) -> bool {
        self.store.db.exists(bid)
    }
    fn filter(&self, criteria: &HashSet<bundlepack::Constraint>) -> Vec<String> {
        let crit = convert_hashset_to_constraints(criteria);
        self.store.db.filter_constraints(crit)
    }
    fn pending(&self) -> Vec<String> {
        self.store
            .db
            .filter_constraints(Constraints::DISPATCH_PENDING)
    }
    fn bundles(&self) -> Vec<BundlePack> {
        let all_ids = self.all_ids();

        let bps = all_ids
            .iter()
            .map(|id| self.store.fs.get_bundle(id).unwrap().into())
            .map(|mut bp: BundlePack| {
                let c = self.store.db.get_constraints(&bp.id).unwrap();
                let c = convert_constraints_to_hashset(c);
                bp.constraints = c;
                bp
            })
            .collect::<Vec<BundlePack>>();
        bps
    }

    fn get_bundle(&self, bpid: &str) -> Option<bp7::Bundle> {
        //info_time!("get_bundle: {}", bpid);
        //{
        self.store.get_bundle(bpid).ok()
        //}
    }

    fn get_metadata(&self, bpid: &str) -> Option<BundlePack> {
        let meta = self.store.db.get_bundle_entry(bpid);
        if meta.is_err() {
            return None;
        }
        let constraints = self.store.db.get_constraints(bpid);
        if constraints.is_err() {
            return None;
        }
        let meta = meta.unwrap();

        let destination = self
            .eid_from_strings(meta.dst_name, meta.dst_service)
            .unwrap_or_default();

        let source = self
            .eid_from_strings(meta.src_name, meta.src_service)
            .unwrap_or_default();
        let bp = BundlePack {
            source,
            destination,
            timestamp: meta.timestamp,
            id: bpid.to_owned(),
            administrative: false,
            size: meta.size as usize,
            constraints: convert_constraints_to_hashset(constraints.unwrap()),
        };
        //debug_time!("get_metadata");
        // {
        /*let bundle = self.store.fs.get_bundle(bpid);
        if bundle.is_err() {
            return None;
        }
        let mut bp: BundlePack = bundle.unwrap().into();
        if let Ok(constraints) = self.store.db.get_constraints(bpid) {
            bp.set_constraints(convert_constraints_to_hashset(constraints));
        }*/

        Some(bp)
        //}
    }
}

impl SneakersBundleStore {
    pub fn new() -> SneakersBundleStore {
        let wd = (*CONFIG.lock()).workdir.clone();
        let store =
            SneakerWorld::open(wd.to_string_lossy().as_ref()).expect("open sneaker bundle store");
        debug!("syncing store fs/db");
        store.sync().expect("sync sneaker bundle store failed");
        SneakersBundleStore { store }
    }
}

impl Default for SneakersBundleStore {
    fn default() -> Self {
        Self::new()
    }
}
fn convert_constraints_to_hashset(constraints: d7sneakers::Constraints) -> HashSet<Constraint> {
    let mut c = HashSet::new();
    if constraints.contains(Constraints::DISPATCH_PENDING) {
        c.insert(Constraint::DispatchPending);
    }
    if constraints.contains(Constraints::FORWARD_PENDING) {
        c.insert(Constraint::ForwardPending);
    }
    if constraints.contains(Constraints::REASSEMBLY_PENDING) {
        c.insert(Constraint::ReassemblyPending);
    }
    if constraints.contains(Constraints::CONTRAINDICATED) {
        c.insert(Constraint::Contraindicated);
    }
    if constraints.contains(Constraints::LOCAL_ENDPOINT) {
        c.insert(Constraint::LocalEndpoint);
    }
    if constraints.contains(Constraints::DELETED) {
        c.insert(Constraint::Deleted);
    }
    c
}

fn convert_hashset_to_constraints(constraints: &HashSet<Constraint>) -> d7sneakers::Constraints {
    let mut c = Constraints::from_bits_truncate(0);
    if constraints.contains(&Constraint::DispatchPending) {
        c.set(Constraints::DISPATCH_PENDING, true);
    }
    if constraints.contains(&Constraint::ForwardPending) {
        c.set(Constraints::FORWARD_PENDING, true);
    }
    if constraints.contains(&Constraint::ReassemblyPending) {
        c.set(Constraints::REASSEMBLY_PENDING, true);
    }
    if constraints.contains(&Constraint::Contraindicated) {
        c.set(Constraints::CONTRAINDICATED, true);
    }
    if constraints.contains(&Constraint::LocalEndpoint) {
        c.set(Constraints::LOCAL_ENDPOINT, true);
    }
    if constraints.contains(&Constraint::Deleted) {
        c.set(Constraints::DELETED, true);
    }
    c
}
