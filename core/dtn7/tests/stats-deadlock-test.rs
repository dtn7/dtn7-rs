use bp7::canonical::new_payload_block;
use bp7::flags::BlockControlFlags;
use bp7::primary::PrimaryBlockBuilder;
use bp7::{Bundle, CreationTimestamp, EndpointID};
use dtn7::core::application_agent::SimpleApplicationAgent;
use dtn7::core::bundlepack::BundlePack;
use dtn7::{CONFIG, DTNCORE};
use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const DELIVERIES: usize = 3000;

fn bundle_to(dst: &EndpointID) -> Bundle {
    let src = EndpointID::try_from("dtn://stats-sender/app".to_string()).unwrap();
    let primary = PrimaryBlockBuilder::default()
        .destination(dst.clone())
        .source(src.clone())
        .report_to(src)
        .creation_timestamp(CreationTimestamp::now())
        .lifetime(Duration::from_secs(60 * 60))
        .build()
        .unwrap();
    Bundle {
        primary,
        canonicals: vec![new_payload_block(
            BlockControlFlags::empty(),
            b"stats".to_vec(),
        )],
    }
}

/// Local delivery and the /status/info statistics both touch STATS and DTNCORE;
/// running them concurrently must not deadlock.
#[test]
fn local_delivery_and_node_statistics_do_not_deadlock() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();

    let local_eid = CONFIG.lock().host_eid.new_endpoint("inbox").unwrap();
    DTNCORE
        .lock()
        .register_application_agent(SimpleApplicationAgent::with(local_eid.clone()).into());

    let delivering = Arc::new(AtomicBool::new(true));
    let (done_tx, done_rx) = mpsc::channel();

    let handle = rt.handle().clone();
    let delivery_done = done_tx.clone();
    let delivery_flag = delivering.clone();
    std::thread::spawn(move || {
        for _ in 0..DELIVERIES {
            let bndl = bundle_to(&local_eid);
            dtn7::store_push_bundle(&bndl).unwrap();
            let bp = BundlePack::from(&bndl);
            handle
                .block_on(dtn7::core::processing::local_delivery(bp))
                .unwrap();
        }
        delivery_flag.store(false, Ordering::SeqCst);
        delivery_done.send("delivery").unwrap();
    });

    std::thread::spawn(move || {
        while delivering.load(Ordering::SeqCst) {
            dtn7::core::node_statistics();
        }
        done_tx.send("statistics").unwrap();
    });

    for _ in 0..2 {
        done_rx
            .recv_timeout(Duration::from_secs(60))
            .expect("deadlock between local delivery and node statistics");
    }
    assert_eq!(dtn7::STATS.lock().delivered as usize, DELIVERIES);
}
