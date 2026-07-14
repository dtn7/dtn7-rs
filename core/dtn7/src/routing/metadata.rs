//! Generic exchange of routing metadata between nodes via administrative records.
//!
//! Routing algorithms such as PRoPHET need to share routing state with encountered peers.
//! This module provides a common mechanism for that:
//! the metadata is encoded as an administrative record that forms the payload of
//! a dedicated administrative bundle unicast to the peer's node endpoint.
//! On delivery, such records are consumed by the daemon and handed to the routing agent as
//! `RoutingNotification::RoutingRecord` instead of being delivered
//! to an application agent (see `core::processing::local_delivery`).
//!
//! Routing protocols that need node-to-node metadata
//! add their record type code to [`ROUTING_RECORD_TYPES`] and reuse
//! [`new_routing_record_bundle`] / [`send_routing_record_bundle`].

use std::time::Duration;

use anyhow::{Result, bail};
use bp7::administrative_record::{AdministrativeRecord, AdministrativeRecordTypeCode};
use bp7::flags::{BlockControlFlags, BundleControlFlags};
use bp7::{Bundle, EndpointID};

use crate::core::bpsec::{self, ProcessingStep, Verdict};
use crate::core::processing::{transmit, validate_bundle};

/// Administrative record type codes that carry routing metadata.
///
/// Records with these codes are dispatched to the routing agent
/// instead of being treated as unknown administrative records.
/// The codes are taken from the private use range 64512-65535 that RFC 9713 reserves.
pub const ROUTING_RECORD_TYPES: &[AdministrativeRecordTypeCode] = &[];

/// Returns true if the given administrative record type code carries routing metadata and
/// should be dispatched to the routing agent instead of being treated as an unknown record.
pub fn is_routing_record(code: AdministrativeRecordTypeCode) -> bool {
    ROUTING_RECORD_TYPES.contains(&code)
}

/// Builds a bundle carrying routing metadata as an administrative record.
///
/// `data` is the already encoded record body,
/// `dst` should be the node endpoint of the peer the record is meant for.
/// Records are marked as administrative payload and must not be fragmented.
/// A hop count block with limit 1 prevents relaying beyond the intended peer.
pub fn new_routing_record_bundle(
    src: &EndpointID,
    dst: &EndpointID,
    record_type: AdministrativeRecordTypeCode,
    data: Vec<u8>,
    lifetime: Duration,
) -> Bundle {
    let flags = BundleControlFlags::BUNDLE_ADMINISTRATIVE_RECORD_PAYLOAD
        | BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED;
    let pblock = bp7::primary::PrimaryBlockBuilder::default()
        .destination(dst.clone())
        .source(src.clone())
        .report_to(src.clone())
        .bundle_control_flags(flags.bits())
        .creation_timestamp(bp7::CreationTimestamp::now())
        .lifetime(lifetime)
        .build()
        .unwrap();
    let mut bndl = bp7::bundle::BundleBuilder::default()
        .primary(pblock)
        .canonicals(vec![
            AdministrativeRecord::Unknown(record_type, data).to_payload(),
            bp7::canonical::new_hop_count_block(2, BlockControlFlags::empty(), 1),
        ])
        .build()
        .unwrap();
    // `validate_bundle` checks the CRCs of the bundle in memory,
    // so they are calculated here and not only during serialization.
    bndl.set_crc(bp7::crc::CRC_32);
    bndl.calculate_crc();
    bndl
}

/// Stores a routing record bundle and starts its transmission without blocking the caller.
///
/// Unlike `send_bundle`, the bundle is validated, secured and stored before this function returns,
/// so a routing agent tracking the record can rely on `store_has_item` for it right away.
/// Only the forwarding itself is spawned, because it calls back into the routing agent.
///
/// Returns an error if the record was not stored, in which case nothing is sent.
pub fn send_routing_record_bundle(mut bndl: Bundle) -> Result<()> {
    validate_bundle(&bndl)?;
    if let Verdict::Drop(_) = bpsec::enforce(ProcessingStep::Transmit, &mut bndl) {
        bail!(
            "routing record bundle {} violates the security policy",
            bndl.id()
        );
    }
    crate::store_push_bundle(&bndl)?;
    tokio::spawn(async move {
        let bid = bndl.id();
        if let Err(err) = transmit(bndl.into()).await {
            log::warn!(
                "Transmission of routing record bundle {} failed: {}",
                bid,
                err
            );
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bp7::administrative_record::BUNDLE_STATUS_REPORT_TYPE_CODE;
    use bp7::flags::BundleValidation;

    /// The last code of the private use range.
    const TEST_RECORD: AdministrativeRecordTypeCode = 65535;

    #[test]
    fn routing_record_bundle_roundtrip() {
        let src: EndpointID = "dtn://node1/".try_into().unwrap();
        let dst: EndpointID = "dtn://node2/".try_into().unwrap();
        let payload = vec![1u8, 2, 3, 4];
        let mut bndl = new_routing_record_bundle(
            &src,
            &dst,
            TEST_RECORD,
            payload.clone(),
            Duration::from_secs(60),
        );

        assert!(bndl.validate().is_ok(), "{:?}", bndl.validate());

        let raw = bndl.to_cbor();
        let mut parsed = Bundle::try_from(raw.as_slice()).expect("roundtrip failed");

        assert!(parsed.crc_valid());
        assert!(parsed.is_administrative_record());
        assert!(
            parsed
                .primary
                .bundle_control_flags
                .flags()
                .contains(BundleControlFlags::BUNDLE_MUST_NOT_FRAGMENTED)
        );
        let hc = parsed
            .extension_block_by_type_mut(bp7::canonical::HOP_COUNT_BLOCK)
            .expect("missing hop count block");
        assert_eq!(hc.hop_count_get(), Some((1, 0)));

        let payload_data = parsed.payload().expect("missing payload").clone();
        let ar: AdministrativeRecord =
            serde_cbor::from_slice(&payload_data).expect("invalid administrative record");
        match ar {
            AdministrativeRecord::Unknown(code, data) => {
                assert_eq!(code, TEST_RECORD);
                assert_eq!(data, payload);
            }
            other => panic!("unexpected administrative record: {:?}", other),
        }
    }

    #[test]
    fn routing_record_types_are_unique_private_use_codes() {
        for (i, code) in ROUTING_RECORD_TYPES.iter().enumerate() {
            assert!(
                (64512..=65535).contains(code),
                "{code} is not a private use code"
            );
            assert!(
                !ROUTING_RECORD_TYPES[..i].contains(code),
                "{code} is allocated twice"
            );
        }
    }

    #[test]
    fn other_records_are_not_routing_records() {
        assert!(!is_routing_record(BUNDLE_STATUS_REPORT_TYPE_CODE));
        assert!(!is_routing_record(0));
    }
}
