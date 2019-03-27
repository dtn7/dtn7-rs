use dtn7::bp::bundle::Block;
use dtn7::bp::{bundle, canonical, crc, dtntime, eid, primary};

fn main() {
    let eid = eid::EndpointID::from("dtn:node1/test".to_string());

    dbg!(&eid);
    dbg!(&eid.get_scheme());
    dbg!(&eid.to_string());
    println!("EID: {}", eid);
    let serialized = serde_json::to_string(&eid).unwrap();
    println!("serialized = {:?}", serialized);

    let serialized = serde_cbor::to_vec(&eid).unwrap();
    println!("serialized = {:?}", serialized);

    let deserialized: eid::EndpointID = serde_cbor::from_slice(&serialized).unwrap();
    println!("deserialized = {:?}", deserialized);

    //dbg!(bundle::Bundle::from(serialized));

    let dst = eid::EndpointID::with_dtn("node2/inbox".to_string());
    let src = eid::EndpointID::with_dtn("node1/123456".to_string());
    //let now = dtntime::CreationTimestamp::with_time_and_seq(dtntime::dtn_time_now(), 0);;
    let day0 = dtntime::CreationTimestamp::with_time_and_seq(dtntime::DTN_TIME_EPOCH, 0);;

    let pblock = primary::PrimaryBlockBuilder::default()
        .destination(dst)
        .source(src.clone())
        .report_to(src)
        .creation_timestamp(day0)
        .lifetime(60 * 60 * 1_000_000)
        .build()
        .unwrap();

    let mut b = bundle::BundleBuilder::default()
        .primary(pblock)
        .canonicals(vec![canonical::new_payload_block(0, b"ABC".to_vec())])
        .build()
        .unwrap();
    dbg!(&b);
    b.set_crc(crc::CRC_16);
    let serialized = b.to_cbor();
    //println!("{:02x?}", serialized);
    println!("{:?}", serialized);
    let binary_bundle = [
        159, 137, 7, 0, 1, 130, 1, 107, 110, 111, 100, 101, 50, 47, 105, 110, 98, 111, 120, 130, 1,
        108, 110, 111, 100, 101, 49, 47, 49, 50, 51, 52, 53, 54, 130, 1, 108, 110, 111, 100, 101,
        49, 47, 49, 50, 51, 52, 53, 54, 130, 0, 0, 26, 214, 147, 164, 0, 66, 54, 202, 134, 1, 0, 0,
        1, 67, 65, 66, 67, 66, 35, 113, 255,
    ];
    assert_eq!(&binary_bundle[..], &serialized[..]);
}
