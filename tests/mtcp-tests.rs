use bp7::{bundle, canonical, crc, dtntime, primary};
use dtn7::cla::mtcp;
use std::convert::TryInto;

#[test]

fn mpdu_encoding() {
    let day0 = dtntime::CreationTimestamp::with_time_and_seq(dtntime::DTN_TIME_EPOCH, 0);
    let pblock = primary::PrimaryBlockBuilder::default()
        .destination("dtn://dest/".try_into().unwrap())
        .source("dtn://src/".try_into().unwrap())
        .report_to("dtn://src/".try_into().unwrap())
        .creation_timestamp(day0)
        .lifetime(std::time::Duration::from_secs(60 * 60))
        .build()
        .unwrap();
    let mut b = bundle::BundleBuilder::default()
        .primary(pblock)
        .canonicals(vec![canonical::new_payload_block(0, b"AAA".to_vec())])
        .build()
        .unwrap();
    b.set_crc(crc::CRC_NO);

    println!("{}", b.to_json());

    println!("{:02x?}", b.to_cbor());

    let mpdu = mtcp::MPDU::new(&b);
    let mpdu_encoded = serde_cbor::to_vec(&mpdu).expect("MPDU encoding error");

    println!("{:02x?}", mpdu_encoded);

    let expected_bytes = vec![
        88, 49, 159, 136, 7, 0, 0, 130, 1, 102, 47, 47, 100, 101, 115, 116, 130, 1, 101, 47, 47,
        115, 114, 99, 130, 1, 101, 47, 47, 115, 114, 99, 130, 0, 0, 26, 0, 54, 238, 128, 133, 1, 1,
        0, 0, 68, 67, 65, 65, 65, 255,
    ];
    println!("{:02x?}", expected_bytes);

    assert_eq!(mpdu_encoded, expected_bytes);
}
