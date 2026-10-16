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
use dtn7::routing::{RoutingAgent, RoutingCmd, RoutingNotifcation};
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
        .send(RoutingCmd::Notify(RoutingNotifcation::IncomingBundle(
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

// The daemon uses process-wide globals and a routing agent bound to the runtime,
// so all checks run sequentially within a single test.
#[tokio::test]
async fn processing_and_routing() {
    unknown_blocks_are_removed_by_index().await;
    bundles_to_dtn_none_are_rejected().await;
    routing_agents_survive_bad_requests().await;
    peers_without_node_name_are_ignored().await;
    deleted_bundles_are_not_accepted_again().await;
}
