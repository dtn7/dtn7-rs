use bp7::{bundle, canonical, crc, dtntime, primary};
use dtn7::cla::mtcp;

#[test]

fn mpdu_encoding() {
    let day0 = dtntime::CreationTimestamp::with_time_and_seq(dtntime::DTN_TIME_EPOCH, 0);
    let pblock = primary::PrimaryBlockBuilder::default()
        .destination(String::from("dtn://dest").into())
        .source(String::from("dtn://src").into())
        .report_to(String::from("dtn://src").into())
        .creation_timestamp(day0)
        .lifetime(60 * 60 * 1_000_000)
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
        0x58, 0x2a, 0x9f, 0x88, 0x07, 0x00, 0x00, 0x82, 0x01, 0x64, 0x64, 0x65, 0x73, 0x74, 0x82,
        0x01, 0x63, 0x73, 0x72, 0x63, 0x82, 0x01, 0x63, 0x73, 0x72, 0x63, 0x82, 0x00, 0x00, 0x1a,
        0xd6, 0x93, 0xa4, 0x00, 0x85, 0x01, 0x01, 0x00, 0x00, 0x43, 0x41, 0x41, 0x41, 0xff,
    ];
    println!("{:02x?}", expected_bytes);

    assert_eq!(mpdu_encoded, expected_bytes);
}
