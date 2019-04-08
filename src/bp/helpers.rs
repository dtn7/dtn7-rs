use super::*;
use crate::bp::dtntime::dtn_time_now;
use rand::Rng;
use std::net::IpAddr;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn rnd_bundle(now: dtntime::CreationTimestamp) -> bundle::Bundle {
    let mut rng = rand::thread_rng();
    let dst_string = format!("node{}/inbox", rng.gen_range(1, 4));
    let src_string = format!("node{}/inbox", rng.gen_range(1, 4));
    let dst = eid::EndpointID::with_dtn(dst_string);
    let src = eid::EndpointID::with_dtn(src_string);
    //let now = dtntime::CreationTimestamp::with_time_and_seq(dtntime::dtn_time_now(), 0);;
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
            canonical::new_bundle_age_block(1, 0, dtn_time_now() as u64),
        ])
        .build()
        .unwrap();
    b.set_crc(crc::CRC_16);
    b.calculate_crc();

    b
}
