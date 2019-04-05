use clap::{App, Arg, SubCommand};
use dtn7::bp::{bundle, canonical, crc, dtntime, eid, helpers::rnd_bundle, primary};
use dtn7::cl::dummy_cl::*;
use dtn7::cl::stcp::*;
use dtn7::core::application_agent::ApplicationAgentData;
use dtn7::core::bundlepack::BundlePack;
use dtn7::core::bundlepack::*;
use dtn7::core::core::DtnCore;
use dtn7::dtnd::daemon::*;
use log::{info, trace, warn};
use pretty_env_logger;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{thread, time};

fn main() {
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUTHORS: &'static str = env!("CARGO_PKG_AUTHORS");

    let matches = App::new("dtn7-rs")
        .version(VERSION)
        .author(AUTHORS)
        .about("A simple Bundle Protocol 7 Daemon for Delay Tolerant Networking")
        .get_matches();

    std::env::set_var("RUST_LOG", "dtn7=debug,dtnd=debug");
    //pretty_env_logger::formatted_timed_builder().init();
    pretty_env_logger::init_timed();

    let dst = eid::EndpointID::with_dtn("node2/inbox".to_string());
    let src = eid::EndpointID::with_dtn("node1/123456".to_string());
    let now = dtntime::CreationTimestamp::with_time_and_seq(dtntime::dtn_time_now(), 0);;
    //let day0 = dtntime::CreationTimestamp::with_time_and_seq(dtntime::DTN_TIME_EPOCH, 0);;

    let pblock = primary::PrimaryBlockBuilder::default()
        .destination(dst)
        .source(src.clone())
        .report_to(src)
        .creation_timestamp(now)
        .lifetime(60 * 60 * 1_000_000)
        .build()
        .unwrap();

    let mut b = bundle::BundleBuilder::default()
        .primary(pblock)
        .canonicals(vec![
            canonical::new_payload_block(0, b"ABC".to_vec()),
            canonical::new_bundle_age_block(
                1,
                0,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis() as u64,
            ),
        ])
        .build()
        .unwrap();
    dbg!(&b);
    b.set_crc(crc::CRC_16);
    let serialized = b.to_cbor();
    //println!("{:02x?}", serialized);
    println!("{:?}", serialized);

    let mut bp = BundlePack::from(b.clone());
    bp.add_constraint(Constraint::ForwardPending);
    dbg!(&bp);

    dbg!(bp.update_bundle_age());
    dbg!(&bp);
    println!("done");

    let mut core = DtnCore::new();

    core.push(b.clone());
    dbg!(&core.store.count());
    core.store.iter().for_each(|e| println!("{:?}", e.id()));

    dbg!(&core.store.has_item(&bp));
    dbg!(&core.store.pending());
    core.store.remove(bp.id());
    core.store.remove(bp.id());
    dbg!(core.store.count());
    dbg!(&core.store.has_item(&bp));

    let aad = ApplicationAgentData::new_with(eid::EndpointID::with_dtn("node2/inbox".to_string()));
    let aad2 =
        ApplicationAgentData::new_with(eid::EndpointID::with_dtn("node2/outbox".to_string()));
    core.register_application_agent(aad);
    core.register_application_agent(aad2.clone());

    println!("Local Application Agent EIDs:");
    dbg!(core.eids());

    //core.unregister_application_agent(aad2);
    //dbg!(core.eids());

    core.push(b.clone());
    core.push(b.clone());

    let ts = core.next_timestamp();
    core.push(rnd_bundle(ts));
    let ts = core.next_timestamp();
    core.push(rnd_bundle(ts));
    let ts = core.next_timestamp();
    core.push(rnd_bundle(ts));

    let sleep_time = time::Duration::from_secs(1);
    thread::sleep(sleep_time);

    let ts = core.next_timestamp();
    core.push(rnd_bundle(ts));
    dbg!(core.bundles());
    core.process();
    dbg!(core.bundles());
    /*
        if let Ok(local_key) = secio::SecioKeyPair::ed25519_generated() {
            let local_peer_id = local_key.to_peer_id(); //PeerId::from(Some(local_key));
            println!("Local peer id: {:?}", local_peer_id);
        }
    */
    let dcl = DummyConversionLayer::new();
    core.cl_list.push(Box::new(dcl));
    let stcp = StcpConversionLayer::new();
    core.cl_list.push(Box::new(stcp));
    info!("starting dtnd");
    start_dtnd(core);
}
