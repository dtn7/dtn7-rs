use bp7::canonical::{
    new_canonical_block, new_hop_count_block, new_payload_block, CanonicalBlock, CanonicalData,
};
use bp7::flags::BlockControlFlags;
use bp7::primary::PrimaryBlockBuilder;
use bp7::{Bundle, CreationTimestamp, EndpointID};
use dtn7::cla::dummy::DummyConvergenceLayer;
use dtn7::cla::ClaSenderTask;
use dtn7::core::bundlepack::BundlePack;
use dtn7::core::peer::{DtnPeer, PeerAddress, PeerType};
use dtn7::routing::epidemic::EpidemicRoutingAgent;
use dtn7::routing::sprayandwait::SprayAndWaitRoutingAgent;
use dtn7::routing::{RoutingAgent, RoutingCmd, RoutingNotification};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

fn eid(s: &str) -> EndpointID {
    EndpointID::try_from(s.to_string()).unwrap()
}

fn bundle(src: &str, dst: EndpointID, canonicals: Vec<CanonicalBlock>) -> Bundle {
    let src = eid(src);
    // the builder refuses dtn:none destinations, so set the destination afterwards
    let mut primary = PrimaryBlockBuilder::default()
        .destination(eid("dtn://placeholder/"))
        .source(src.clone())
        .report_to(src)
        .creation_timestamp(CreationTimestamp::now())
        .lifetime(Duration::from_secs(60 * 60))
        .build()
        .unwrap();
    primary.destination = dst;
    Bundle {
        primary,
        canonicals,
    }
}

fn unknown_block(block_type: u64, block_number: u64, flags: BlockControlFlags) -> CanonicalBlock {
    new_canonical_block(
        block_type,
        block_number,
        flags.bits(),
        CanonicalData::Unknown(vec![0xab; 4]),
    )
}

async fn sender_for_bundle(
    agent: &Sender<RoutingCmd>,
    bp: BundlePack,
) -> Option<(Vec<ClaSenderTask>, bool)> {
    let (tx, rx) = oneshot::channel();
    agent.send(RoutingCmd::SenderForBundle(bp, tx)).await.ok()?;
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .ok()?
        .ok()
}

/// Sends a GetData request whose requester is already gone, e.g., due to a timeout.
async fn get_data_with_dropped_requester(agent: &Sender<RoutingCmd>) {
    let (tx, rx) = oneshot::channel();
    drop(rx);
    agent
        .send(RoutingCmd::GetData(String::new(), tx))
        .await
        .unwrap();
}

/// Block type of BPSec integrity blocks (RFC 9172, 3.7), as bp7 only defines it with
/// its bpsec feature.
const INTEGRITY_BLOCK: u64 = 11;

/// Reserved block types are not processed by this node, so their block processing
/// control flags apply, while BPSec blocks are only known with the bpsec feature.
async fn reserved_and_security_block_types() {
    let bndl = bundle(
        "dtn://blocktype-sender/app",
        eid("dtn://blocktype-receiver/app"),
        vec![
            unknown_block(2, 3, BlockControlFlags::BLOCK_REMOVE),
            unknown_block(INTEGRITY_BLOCK, 4, BlockControlFlags::BLOCK_REMOVE),
            new_payload_block(BlockControlFlags::empty(), b"payload".to_vec()),
        ],
    );
    let bid = bndl.id();

    dtn7::core::processing::receive(bndl).await.unwrap();

    let stored = dtn7::store_get_bundle(&bid).expect("bundle not stored");
    let block_types: Vec<u64> = stored.canonicals.iter().map(|c| c.block_type).collect();
    assert!(!block_types.contains(&2), "reserved block type kept");
    assert_eq!(
        block_types.contains(&INTEGRITY_BLOCK),
        cfg!(feature = "bpsec"),
        "unexpected handling of the BIB: {:?}",
        block_types
    );
}

/// Unknown blocks flagged for removal must be removed, all others kept.
async fn unknown_blocks_are_removed_by_index() {
    let bndl = bundle(
        "dtn://remove-sender/app",
        eid("dtn://remove-receiver/app"),
        vec![
            new_hop_count_block(2, BlockControlFlags::empty(), 32),
            unknown_block(200, 3, BlockControlFlags::BLOCK_REMOVE),
            unknown_block(201, 4, BlockControlFlags::BLOCK_REMOVE),
            unknown_block(202, 5, BlockControlFlags::empty()),
            new_payload_block(BlockControlFlags::empty(), b"payload".to_vec()),
        ],
    );
    let bid = bndl.id();

    dtn7::core::processing::receive(bndl).await.unwrap();

    let stored = dtn7::store_get_bundle(&bid).expect("bundle not stored");
    let mut block_types: Vec<u64> = stored.canonicals.iter().map(|c| c.block_type).collect();
    block_types.sort_unstable();
    assert_eq!(
        block_types,
        vec![bp7::PAYLOAD_BLOCK, bp7::canonical::HOP_COUNT_BLOCK, 202]
    );
}

/// Bundles without a destination node must be rejected, not stored.
async fn bundles_to_dtn_none_are_rejected() {
    let bndl = bundle(
        "dtn://none-sender/app",
        EndpointID::none(),
        vec![new_payload_block(
            BlockControlFlags::empty(),
            b"payload".to_vec(),
        )],
    );
    let bid = bndl.id();
    assert!(dtn7::core::processing::receive(bndl).await.is_err());
    assert!(!dtn7::store_has_item(&bid));
}

/// Routing agents must survive bundles to dtn:none and requesters that went away.
async fn routing_agents_survive_bad_requests() {
    dtn7::cla_add(DummyConvergenceLayer::new(None).into());
    dtn7::peers_add(DtnPeer::new(
        eid("dtn://routing-peer/"),
        PeerAddress::Ip("127.0.0.1".parse().unwrap()),
        PeerType::Static,
        None,
        vec![("dummy".into(), None)],
        HashMap::new(),
    ));

    let to_none = bundle(
        "dtn://routing-sender/app",
        EndpointID::none(),
        vec![new_payload_block(
            BlockControlFlags::empty(),
            b"payload".to_vec(),
        )],
    );
    let to_none_bp = BundlePack::from(&to_none);
    let regular = bundle(
        "dtn://routing-sender/app",
        eid("dtn://routing-receiver/app"),
        vec![new_payload_block(
            BlockControlFlags::empty(),
            b"payload".to_vec(),
        )],
    );
    let regular_bp = BundlePack::from(&regular);

    let epidemic = EpidemicRoutingAgent::new().channel();
    let (clas, _) = sender_for_bundle(&epidemic, to_none_bp.clone())
        .await
        .expect("epidemic: no reply for bundle to dtn:none");
    assert_eq!(clas.len(), 1);
    get_data_with_dropped_requester(&epidemic).await;
    let (clas, _) = sender_for_bundle(&epidemic, regular_bp.clone())
        .await
        .expect("epidemic: routing agent died");
    assert_eq!(clas.len(), 1);

    let spray = SprayAndWaitRoutingAgent::new().channel();
    spray
        .send(RoutingCmd::Notify(RoutingNotification::IncomingBundle(
            to_none,
        )))
        .await
        .unwrap();
    // a foreign bundle has a single copy left, which triggers the direct delivery check
    let (clas, _) = sender_for_bundle(&spray, to_none_bp)
        .await
        .expect("sprayandwait: no reply for bundle to dtn:none");
    assert!(clas.is_empty());
    get_data_with_dropped_requester(&spray).await;
    assert!(
        sender_for_bundle(&spray, regular_bp).await.is_some(),
        "sprayandwait: routing agent died"
    );
}

/// Peers without a node name (e.g., from a beacon with a dtn:none source) are ignored.
async fn peers_without_node_name_are_ignored() {
    let before = dtn7::peers_count();
    let peer = DtnPeer::new(
        EndpointID::none(),
        PeerAddress::Ip("127.0.0.1".parse().unwrap()),
        PeerType::Dynamic,
        None,
        vec![("dummy".into(), None)],
        HashMap::new(),
    );
    assert!(!dtn7::peers_add(peer));
    assert_eq!(dtn7::peers_count(), before);
}

/// Bundles deleted from the store must not be accepted again.
async fn deleted_bundles_are_not_accepted_again() {
    let bndl = bundle(
        "dtn://deleted-sender/app",
        eid("dtn://deleted-receiver/app"),
        vec![new_payload_block(
            BlockControlFlags::empty(),
            b"payload".to_vec(),
        )],
    );
    let bid = bndl.id();
    dtn7::core::processing::receive(bndl.clone()).await.unwrap();
    assert!(dtn7::store_has_item(&bid));
    dtn7::store_remove(&bid).unwrap();

    let dups = dtn7::STATS.lock().dups;
    dtn7::core::processing::receive(bndl).await.unwrap();
    assert!(
        !dtn7::store_has_item(&bid),
        "deleted bundle was stored again"
    );
    assert!(dtn7::store_is_known(&bid));
    assert_eq!(dtn7::STATS.lock().dups, dups + 1);
}

fn payload() -> CanonicalBlock {
    new_payload_block(BlockControlFlags::empty(), b"payload".to_vec())
}

fn valid_bundle() -> Bundle {
    bundle(
        "dtn://valid-sender/app",
        eid("dtn://valid-receiver/app"),
        vec![
            new_hop_count_block(2, BlockControlFlags::empty(), 32),
            payload(),
        ],
    )
}

/// Received bundles are fully validated (RFC 9171, 4).
async fn invalid_bundles_are_rejected() {
    use dtn7::core::processing::validate_bundle;

    assert!(validate_bundle(&valid_bundle()).is_ok());

    let mut with_crc = valid_bundle();
    with_crc.set_crc(bp7::crc::CRC_32);
    with_crc.calculate_crc();
    assert!(validate_bundle(&with_crc).is_ok());

    // primary block modified after the CRC was calculated
    let mut corrupted = with_crc.clone();
    corrupted.primary.lifetime = Duration::from_secs(1);
    assert!(validate_bundle(&corrupted).is_err());

    let mut payload_not_last = valid_bundle();
    payload_not_last.canonicals.reverse();
    assert!(validate_bundle(&payload_not_last).is_err());

    let mut duplicate_block_numbers = valid_bundle();
    duplicate_block_numbers
        .canonicals
        .insert(0, unknown_block(200, 2, BlockControlFlags::empty()));
    assert!(validate_bundle(&duplicate_block_numbers).is_err());

    let no_payload = bundle(
        "dtn://invalid-sender/app",
        eid("dtn://invalid-receiver/app"),
        vec![new_hop_count_block(2, BlockControlFlags::empty(), 32)],
    );
    assert!(validate_bundle(&no_payload).is_err());

    // invalid bundles are neither stored nor processed
    let bid = payload_not_last.id();
    assert!(dtn7::core::processing::receive(payload_not_last)
        .await
        .is_err());
    assert!(!dtn7::store_is_known(&bid));
}

/// Bundles from nodes without a clock must carry a bundle age block (RFC 9171, 4.3.1).
async fn clockless_bundles_require_bundle_age_block() {
    use dtn7::core::processing::validate_bundle;

    let mut with_age = valid_bundle();
    with_age.primary.creation_timestamp = CreationTimestamp::with_time_and_seq(0, 1);
    with_age.canonicals.insert(
        0,
        bp7::canonical::new_bundle_age_block(3, BlockControlFlags::empty(), 0),
    );
    assert!(validate_bundle(&with_age).is_ok());

    let mut without_age = valid_bundle();
    without_age.primary.creation_timestamp = CreationTimestamp::with_time_and_seq(0, 2);
    assert!(validate_bundle(&without_age).is_err());
}

/// Bundles without primary block CRC and BIB are only rejected with strict validation.
async fn missing_primary_crc_is_rejected_only_when_strict() {
    use dtn7::core::processing::validate_bundle;

    let without_crc = valid_bundle();
    let mut with_crc = valid_bundle();
    with_crc.set_crc(bp7::crc::CRC_32);
    with_crc.calculate_crc();

    assert!(validate_bundle(&without_crc).is_ok());
    assert!(validate_bundle(&with_crc).is_ok());

    dtn7::CONFIG.lock().strict_validation = true;
    let strict_without_crc = validate_bundle(&without_crc);
    let strict_with_crc = validate_bundle(&with_crc);
    dtn7::CONFIG.lock().strict_validation = false;

    // bp7 only checks the primary block CRC with its bpsec feature
    assert_eq!(strict_without_crc.is_err(), cfg!(feature = "bpsec"));
    assert!(strict_with_crc.is_ok());
}

/// Received bundles are checked against the security rules: bundles with a valid BIB are
/// stored without it, tampered or unsigned bundles are deleted.
#[cfg(feature = "bpsec")]
async fn security_rules_are_enforced_on_reception() {
    use dtn7::core::bpsec::config::*;

    const KEY: &str = "00112233445566778899aabbccddeeff";
    let keys = tempfile::tempdir().unwrap();
    std::fs::write(keys.path().join("hmac.key"), KEY).unwrap();
    let config = BpsecConfig {
        keys: Some(keys.path().into()),
        rules: vec![SecurityRule {
            name: "accept-payload".into(),
            role: SecurityRole::Acceptor,
            source: "dtn://bpsec-sender/**".into(),
            destination: "**".into(),
            operation: SecurityOperation::Bib,
            targets: vec![SecurityTarget::Payload],
            key: "hmac".into(),
            context: SecurityContext::default(),
            on_failure: FailureAction::Drop,
        }],
    };
    dtn7::core::bpsec::init(&config).unwrap();

    let signed = |payload: &[u8]| {
        let mut bndl = bundle(
            "dtn://bpsec-sender/app",
            eid("dtn://bpsec-receiver/app"),
            vec![new_payload_block(
                BlockControlFlags::empty(),
                payload.to_vec(),
            )],
        );
        let key = bp7::helpers::unhexify(KEY).unwrap();
        bp7::bpsec::sign(
            &mut bndl,
            &[1],
            &key,
            eid("dtn://bpsec-sender/"),
            Default::default(),
        )
        .unwrap();
        bndl
    };
    let is_deleted = |bid: &str| dtn7::store_is_known(bid) && !dtn7::store_has_item(bid);

    let valid = signed(b"valid");
    let bid = valid.id();
    dtn7::core::processing::receive(valid).await.unwrap();
    assert!(!is_deleted(&bid));
    let stored = dtn7::store_get_bundle(&bid).unwrap();
    assert!(stored.decode_bibs().is_empty(), "accepted BIB was kept");

    let mut tampered = signed(b"tampered");
    tampered
        .extension_block_by_type_mut(bp7::PAYLOAD_BLOCK)
        .unwrap()
        .set_data(CanonicalData::Data(b"changed".to_vec()));
    let bid = tampered.id();
    dtn7::core::processing::receive(tampered).await.unwrap();
    assert!(is_deleted(&bid), "tampered bundle was not deleted");

    let unsigned = bundle(
        "dtn://bpsec-sender/app",
        eid("dtn://bpsec-receiver/app"),
        vec![payload()],
    );
    let bid = unsigned.id();
    dtn7::core::processing::receive(unsigned).await.unwrap();
    assert!(is_deleted(&bid), "unsigned bundle was not deleted");

    let status = dtn7::core::bpsec::status();
    assert!(status.active);
    assert_eq!(status.keys, 1);
    let statistics = status.rules[0].statistics;
    assert_eq!(
        (statistics.accepted, statistics.failed, statistics.missing),
        (1, 1, 1)
    );
    assert_eq!(statistics.dropped, 2);

    dtn7::core::bpsec::init(&BpsecConfig::default()).unwrap();
    assert!(!dtn7::core::bpsec::status().active);
}

// The daemon uses process-wide globals and a routing agent bound to the runtime,
// so all checks run sequentially within a single test.
#[tokio::test]
async fn processing_and_routing() {
    unknown_blocks_are_removed_by_index().await;
    reserved_and_security_block_types().await;
    bundles_to_dtn_none_are_rejected().await;
    routing_agents_survive_bad_requests().await;
    peers_without_node_name_are_ignored().await;
    deleted_bundles_are_not_accepted_again().await;
    invalid_bundles_are_rejected().await;
    clockless_bundles_require_bundle_age_block().await;
    missing_primary_crc_is_rejected_only_when_strict().await;
    #[cfg(feature = "bpsec")]
    security_rules_are_enforced_on_reception().await;
}
