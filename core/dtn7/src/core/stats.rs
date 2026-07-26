use bp7::Bundle;
use serde::{Deserialize, Serialize};

use crate::CONFIG;
use crate::routing::metadata::carries_routing_record;

/// Traffic class of a bundle, used to tell routing overhead apart from payload traffic.
///
/// The classes partition all bundles, so the counts of a [`ClassCounts`]
/// always add up to the flat counter they break down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleClass {
    /// Payload bundle addressed to a singleton endpoint.
    Unicast,
    /// Payload bundle addressed to a non-singleton endpoint.
    Group,
    /// Administrative record carrying routing metadata.
    Routing,
    /// Any other administrative record, such as a bundle status report.
    Admin,
}

impl BundleClass {
    /// Classifies a bundle by its administrative record type and destination endpoint.
    pub fn of(bundle: &Bundle) -> BundleClass {
        if bundle.is_administrative_record() {
            if carries_routing_record(bundle) {
                BundleClass::Routing
            } else {
                BundleClass::Admin
            }
        } else if bundle.primary.destination.is_non_singleton() {
            BundleClass::Group
        } else {
            BundleClass::Unicast
        }
    }
}

/// Number of bundles of one traffic class and their total encoded size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClassCount {
    /// Total number of bundles of this class.
    pub bundles: u64,
    /// Total size of the encoded bundles in bytes.
    pub bytes: u64,
}

/// Breakdown of a bundle counter by [`BundleClass`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClassCounts {
    pub unicast: ClassCount,
    pub group: ClassCount,
    pub routing: ClassCount,
    pub admin: ClassCount,
}

impl ClassCounts {
    /// Records one bundle of `class` with an encoded size of `bytes`.
    pub fn count(&mut self, class: BundleClass, bytes: usize) {
        let entry = match class {
            BundleClass::Unicast => &mut self.unicast,
            BundleClass::Group => &mut self.group,
            BundleClass::Routing => &mut self.routing,
            BundleClass::Admin => &mut self.admin,
        };
        entry.bundles += 1;
        entry.bytes += bytes as u64;
    }

    /// Sum over all classes.
    pub fn total(&self) -> ClassCount {
        ClassCount {
            bundles: self.unicast.bundles
                + self.group.bundles
                + self.routing.bundles
                + self.admin.bundles,
            bytes: self.unicast.bytes + self.group.bytes + self.routing.bytes + self.admin.bytes,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bp7::EndpointID;
    use bp7::administrative_record::{AdministrativeRecord, BUNDLE_STATUS_REPORT_TYPE_CODE};

    use super::*;
    use crate::routing::metadata::{PROPHET_RIB_RECORD, new_routing_record_bundle};

    fn eid(s: &str) -> EndpointID {
        s.try_into().unwrap()
    }

    fn routing_record() -> Bundle {
        new_routing_record_bundle(
            &eid("dtn://node1/"),
            &eid("dtn://node2/"),
            PROPHET_RIB_RECORD,
            vec![1, 2, 3],
            Duration::from_secs(60),
        )
    }

    #[test]
    fn payload_bundles_are_classified_by_destination() {
        let unicast = bp7::bundle::new_std_payload_bundle(
            eid("dtn://node1/app"),
            eid("dtn://node2/app"),
            b"data".to_vec(),
        );
        assert_eq!(BundleClass::of(&unicast), BundleClass::Unicast);

        let group = bp7::bundle::new_std_payload_bundle(
            eid("dtn://node1/app"),
            eid("dtn://node2/~topic"),
            b"data".to_vec(),
        );
        assert_eq!(BundleClass::of(&group), BundleClass::Group);
    }

    #[test]
    fn routing_records_are_not_counted_as_plain_administrative_records() {
        assert_eq!(BundleClass::of(&routing_record()), BundleClass::Routing);

        let mut report = routing_record();
        report.canonicals[0] =
            AdministrativeRecord::Unknown(BUNDLE_STATUS_REPORT_TYPE_CODE, vec![1, 2, 3])
                .to_payload();
        assert_eq!(BundleClass::of(&report), BundleClass::Admin);
    }

    #[test]
    fn classification_survives_a_wire_roundtrip() {
        let raw = routing_record().to_cbor();
        let parsed = Bundle::try_from(raw.as_slice()).expect("roundtrip failed");
        assert_eq!(BundleClass::of(&parsed), BundleClass::Routing);
    }

    #[test]
    fn class_counts_partition_the_total() {
        let mut counts = ClassCounts::default();
        counts.count(BundleClass::Routing, 100);
        counts.count(BundleClass::Routing, 50);
        counts.count(BundleClass::Group, 400);
        counts.count(BundleClass::Unicast, 200);

        assert_eq!(counts.routing.bundles, 2);
        assert_eq!(counts.routing.bytes, 150);
        assert_eq!(counts.admin, ClassCount::default());
        assert_eq!(
            counts.total(),
            ClassCount {
                bundles: 4,
                bytes: 750
            }
        );
    }
}
