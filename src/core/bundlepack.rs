use crate::store_remove;
use crate::store_update_metadata;
use anyhow::Result;
use bp7::{Bundle, EndpointID};
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Constraint is a retention constraint as defined in the subsections of the
/// fifth chapter of draft-ietf-dtn-bpbis-12.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)]
pub enum Constraint {
    /// DispatchPending is assigned to a bundle if its dispatching is pending.
    DispatchPending,
    /// ForwardPending is assigned to a bundle if its forwarding is pending.
    ForwardPending,
    /// ReassemblyPending is assigned to a fragmented bundle if its reassembly is
    /// pending.
    ReassemblyPending,
    /// Contraindicated is assigned to a bundle if it could not be delivered and
    /// was moved to the contraindicated stage. This Constraint was not defined
    /// in draft-ietf-dtn-bpbis-12, but seemed reasonable for this implementation.
    Contraindicated,

    /// LocalEndpoint is assigned to a bundle after delivery to a local endpoint.
    /// This constraint demands storage until the endpoint removes this constraint.
    LocalEndpoint,

    /// This bundle has been deleted, only the meta data is kept to prevent
    /// resubmission in the future.
    Deleted,
}

impl fmt::Display for Constraint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// BundlePack is a set of a bundle, it's creation or reception time stamp and
/// a set of constraints used in the process of delivering this bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundlePack {
    pub source: EndpointID,
    pub destination: EndpointID,
    /// time at which bundle was received at this node in unix time as milliseconds
    pub received_time: u64,
    /// time at which bundle was created in dtntime
    pub creation_time: u64,
    pub id: String,
    pub administrative: bool,
    pub size: usize,
    pub constraints: HashSet<Constraint>,
}

impl fmt::Display for BundlePack {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {:?}", self.id(), self.constraints)
    }
}

/// Create from a given bundle.
impl From<Bundle> for BundlePack {
    fn from(mut bundle: Bundle) -> Self {
        let bid = bundle.id();
        let size = bundle.to_cbor().len();
        let source = bundle.primary.source.clone();
        let destination = bundle.primary.destination.clone();
        BundlePack {
            source,
            destination,
            received_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64,
            creation_time: bundle.primary.creation_timestamp.dtntime(),
            id: bid,
            administrative: bundle.is_administrative_record(),
            size,
            constraints: HashSet::new(),
        }
    }
}
/// Create from a given bundle.
impl From<&Bundle> for BundlePack {
    fn from(bundle: &Bundle) -> Self {
        let bid = bundle.id();
        let size = bundle.clone().to_cbor().len();
        let source = bundle.primary.source.clone();
        let destination = bundle.primary.destination.clone();
        BundlePack {
            source,
            destination,
            received_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_millis() as u64,
            creation_time: bundle.primary.creation_timestamp.dtntime(),
            id: bid,
            administrative: bundle.is_administrative_record(),
            size,
            constraints: HashSet::new(),
        }
    }
}

impl BundlePack {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn sync(&self) -> Result<()> {
        if !self.has_constraints() {
            warn!("not constraints, removing bundle from store {}", self.id());
            store_remove(self.id());
        } else {
            // TODO: add update logic
            store_update_metadata(self)?;
        }
        Ok(())
    }
    pub fn has_receiver(&self) -> bool {
        self.destination != EndpointID::none()
    }
    pub fn has_constraint(&self, constraint: Constraint) -> bool {
        self.constraints.contains(&constraint)
    }
    pub fn has_constraints(&self) -> bool {
        !self.constraints.is_empty()
    }
    pub fn add_constraint(&mut self, constraint: Constraint) {
        self.constraints.insert(constraint);
    }
    pub fn remove_constraint(&mut self, constraint: Constraint) {
        self.constraints.remove(&constraint);
    }
    pub fn clear_constraints(&mut self) {
        let local_set = self.has_constraint(Constraint::LocalEndpoint);

        self.constraints.clear();

        if local_set {
            self.add_constraint(Constraint::LocalEndpoint);
        }
    }
    pub fn set_constraints(&mut self, constraints: HashSet<Constraint>) {
        self.constraints = constraints;
    }
    pub fn to_cbor(&self) -> bp7::ByteBuffer {
        serde_cbor::to_vec(self).expect("unexpected error converting BundlePack to cbor buffer")
    }
}

/// Create from a given bundle.
impl From<&[u8]> for BundlePack {
    fn from(buf: &[u8]) -> Self {
        serde_cbor::from_slice(buf).expect("unexpected error converting cbor buffer to BundlePack")
    }
}
