use serde::{Deserialize, Serialize};

use crate::CONFIG;

/// CCSDS 734.20-O-1 Bundle Protocol Orange Book - Annex C Managed Information
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NodeStats {
    pub registrations: Vec<RegistrationInformation>,
    pub node_state: NodeStateInformation,
    pub error_info: ErrorInformation,
    pub bundles: BundleStateInformation,
}

impl NodeStats {
    pub fn new() -> Self {
        let mut mib = NodeStats {
            registrations: Vec::new(),
            node_state: NodeStateInformation::default(),
            error_info: ErrorInformation::default(),
            bundles: BundleStateInformation::default(),
        };
        mib.node_state.administrative_eid = CONFIG.lock().host_eid.clone().to_string();
        mib.node_state.bp_versions = vec![7]; // Bundle Protocol version - fixed to 7 for now

        // mib.error_info.failed_forwards_bundle_count = (*STATS.lock()).failed;
        mib
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureAction {
    Abandon = 0,
    Defer = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
/// CCSDS Bundle Protocol Orange Book - Annex C Bundle State Information Table C-1
pub struct BundleStateInformation {
    pub forward_pending_bundle_count: u64,
    pub dispatch_pending_bundle_count: u64,
    pub reassembly_pending_bundle_count: u64,
    pub bundles_created: u64,
    pub bundles_stored: u64,
    pub bundles_fragmented: u64,
    pub fragments_created: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
/// CCSDS Bundle Protocol Orange Book - Annex C Error and Reporting Information Table C-2
pub struct ErrorInformation {
    /// The number of bundles/bytes that have experienced a forwarding failure at this node.
    pub failed_forwards_bundle_count: u64,
    /// The number of bundles/bytes whose delivery has been abandoned at this node.
    pub abandoned_delivery_bundle_count: u64,
    /// The number of bundles/bytes discarded at this node.
    pub discarded_bundle_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
/// CCSDS Bundle Protocol Orange Book - Annex C Node State Information Table C-4
pub struct NodeStateInformation {
    /// The EID that uniquely and permanently identifies this node’s administrative endpoint.
    pub administrative_eid: String,
    /// The number(s) of the version(s) of the BP supported at this node.
    pub bp_versions: Vec<u8>, // Bundle Protocol version - fixed to 7 for now
    /// The number of kilobytes of storage allocated to bundle retention at this node and not currently occupied by bundles.
    pub storage_available: u64,
    /// The most recent time at which the operation of this node was started or restarted.
    pub last_up_time: u64,
    /// The number of different endpoints in which this node has been registered since it was started or restarted.
    pub registration_count: u64, // optional
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// CCSDS Bundle Protocol Orange Book - Annex C Registration Information Table C-4
pub struct RegistrationInformation {
    /// The EID of the endpoint for which this registration applies.
    pub eid: String,
    /// The current state of the EID, at the time the managed information was queried. True - ACTIVE, False - PASSIVE
    pub active: bool,
    /// Whether this EID is a singleton EID.
    pub singleton: bool,
    /// The default action to be taken when delivery is not possible.
    /// One of: ABANDON or DEFER.
    pub default_failure_action: FailureAction,
}
