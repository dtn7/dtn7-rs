use bp7::eid::IpnAddress;
use bp7::EndpointID;
use dtn7::cla::CLAsAvailable;
use dtn7::core::*;
use dtn7::get_sequence;
use dtn7::ipnd::{beacon::*, services::ServiceBlock};
use dtn7::CONFIG;
use rand::thread_rng;
use rand::Rng;
use std::{convert::TryFrom, time::SystemTime};
use std::{thread, time::Duration};

/// Prints to stdout the content of a beacon without ServiceBlock & BeaconPeriod
#[test]
pub fn beacon_without_config() {
    let eid = EndpointID::new();
    let beacon = Beacon::new(eid);
    println!("{}", beacon);
}

/// Prints to stdout the content of a beacon with ServiceBlock & BeaconPeriod
#[test]
pub fn beacon_with_config() {
    let eid = EndpointID::new();
    let beacon_period = Duration::from_secs(5);
    let mut service_block = ServiceBlock::new();
    let first = (String::from("mtcp"), Some(20));
    service_block.add_cla(&first.0, &first.1);
    let (tag, service) = ServiceBlock::build_custom_service(191, "75")
        .expect("Error while building custom service: ");
    service_block.add_custom_service(tag, &service);
    let beacon = Beacon::with_config(eid, service_block, Some(beacon_period));
    println!("{}", beacon);
}

#[test]
pub fn bsn_overflow() {
    (*CONFIG.lock())
        .discovery_destinations
        .insert("Node1".to_string(), u32::MAX - 1);
    assert_eq!(get_sequence(&"Node1".to_string()), u32::MAX - 1);
    (*CONFIG.lock()).update_beacon_sequence_number(&"Node1".to_string());
    assert_eq!(get_sequence(&"Node1".to_string()), u32::MAX);
    (*CONFIG.lock()).update_beacon_sequence_number(&"Node1".to_string());
    assert_eq!(get_sequence(&"Node1".to_string()), 0);
}

/// Serializes a Beacon without ServiceBlock & BeaconPeriod
///
/// Prints content of Byte array to stdout
///
/// Then deserializes said Beacon
///
/// and prints the Beacon created by Deserialization to stdout
#[test]
pub fn plain_serialization() {
    let eid = EndpointID::try_from("dtn://n1/").unwrap();
    let beacon = Beacon::new(eid);
    let serialized = serde_cbor::to_vec(&beacon);
    let unwrapped = serialized.expect("Error");

    for e in &unwrapped {
        print!("{:02x?} ", e);
    }
    println!("Beacon size: {}", &unwrapped.len());
    println!();

    let deserialized: Beacon = match serde_cbor::from_slice(&unwrapped) {
        Ok(pkt) => pkt,
        Err(e) => {
            println!("{}", e);
            Beacon::new(EndpointID::new())
        }
    };

    println!("{}", deserialized);
    assert_eq!(beacon, deserialized);
}

/// Serializes a Beacon with ServiceBlock but without BeaconPeriod
///
/// Prints content of Byte array to stdout
///
/// Then deserializes said Beacon
///
/// and prints the Beacon created by Deserialization to stdout
#[test]
pub fn serialization_with_service_block() {
    let eid = EndpointID::try_from("dtn://n1/").unwrap();
    let mut service_block = ServiceBlock::new();
    let first = (String::from("mtcp"), Some(3003));
    service_block.add_cla(&first.0, &first.1);
    let (tag, service) = ServiceBlock::build_custom_service(191, "75")
        .expect("Error while building custom service: ");
    service_block.add_custom_service(tag, &service);
    let third = (String::from("http"), None);
    service_block.add_cla(&third.0, &third.1);
    let beacon = Beacon::with_config(eid, service_block, None);

    let serialized = serde_cbor::to_vec(&beacon);
    let unwrapped = serialized.expect("Error");

    for e in &unwrapped {
        print!("{:02x?} ", e);
    }

    println!();
    println!("{}", &unwrapped.len());
    println!();

    let deserialized: Beacon = match serde_cbor::from_slice(&unwrapped) {
        Ok(pkt) => pkt,
        Err(e) => panic!("deserialization error: {}", e),
    };

    println!("{}", &deserialized);

    assert_eq!(beacon, deserialized);
}

/// Serializes a Beacon with BeaconPeriod but without ServiceBlock
///
/// Prints content of Byte array to stdout
///
/// Then deserializes said Beacon
///
/// and prints the Beacon created by Deserialization to stdout
#[test]
pub fn serialization_with_beacon_period() {
    let eid = EndpointID::try_from("dtn://n1/").unwrap();
    let beacon = Beacon::with_config(eid, ServiceBlock::new(), Some(Duration::from_secs(5)));

    let serialized = serde_cbor::to_vec(&beacon);
    let unwrapped = serialized.expect("Error");

    for e in &unwrapped {
        print!("{:02x?} ", e);
    }

    println!();
    println!("Beacon size: {}", &unwrapped.len());

    let deserialized: Beacon = match serde_cbor::from_slice(&unwrapped) {
        Ok(pkt) => pkt,
        Err(e) => {
            println!("{}", e);
            Beacon::new(EndpointID::new())
        }
    };
    println!("{}", deserialized);
    assert_eq!(beacon, deserialized);
}

/// Serializes a Beacon with ServiceBlock & BeaconPeriod
///
/// Prints content of Byte array to stdout
///
/// Then deserializes said Beacon
///
/// and prints the Beacon created by Deserialization to stdout
#[test]
pub fn serialization_with_full_config() {
    let eid = EndpointID::try_from("dtn://n1/").unwrap();
    let mut service_block = ServiceBlock::new();
    let first = (String::from("mtcp"), Some(4556));
    service_block.add_cla(&first.0, &first.1);
    let beacon_period = Some(Duration::from_secs(2));
    let beacon = Beacon::with_config(eid, service_block, beacon_period);

    let serialized = serde_cbor::to_vec(&beacon);
    let unwrapped = serialized.expect("Error");

    for e in &unwrapped {
        print!("{:02x?} ", e);
    }

    println!();
    println!("Packet size: {}", unwrapped.len());

    let deserialized: Beacon = match serde_cbor::from_slice(&unwrapped) {
        Ok(pkt) => pkt,
        Err(e) => {
            println!("{}", e);
            Beacon::new(EndpointID::new())
        }
    };

    println!("{}", deserialized);
    assert_eq!(beacon, deserialized);
}

/// Checks if 5000 randomly generated beacons will be serialized and deserialized correctly
#[test]
pub fn check_if_deserialized_is_equal_to_before() {
    let mut beacons: Vec<Beacon> = Vec::new();
    let mut serialized: Vec<Vec<u8>> = Vec::new();

    for _i in 0..5001 {
        beacons.push(rnd_beacon());
    }

    for x in &beacons {
        serialized.push(serde_cbor::to_vec(x).expect("A problem occurred while serializing"));
    }

    let mut deserialized: Vec<Beacon> = Vec::new();
    for x in serialized {
        deserialized.push(serde_cbor::from_slice::<Beacon>(&x).expect("Something bad happened"));
    }

    for i in 0..beacons.len() {
        assert_eq!(beacons[i], deserialized[i]);
    }
}

/// Computes the amount of time needed to randomly generate one beacon
#[test]
pub fn check_generation_time() {
    let now = SystemTime::now();
    println!(
        "Starting beacon creation at {}ns",
        now.elapsed().unwrap().as_nanos()
    );
    let _beacon = rnd_beacon();
    let elapsed = now.elapsed().unwrap();
    println!(
        "Beacon creation finished after {}ns, or {}s",
        elapsed.as_nanos(),
        elapsed.as_secs()
    );
}

/// Computes the amount of time needed to generate 5000 random beacons
#[test]
pub fn check_time_for_generation_of_5000_beacons() {
    let now = SystemTime::now();
    let first = now.elapsed().unwrap().as_nanos();
    println!("Starting beacon creation at {}ns", first);
    for _i in 0..5001 {
        rnd_beacon();
    }
    let elapsed = now.elapsed().unwrap();
    let second = elapsed.as_nanos();
    let secs = elapsed.as_secs();
    println!(
        "Beacon creation finished at {}ns, or {}s.\nIt took {}ns to create 5000 beacons",
        second,
        secs,
        second - first
    );
}

/// Computes the amount of time needed to serialize 5000 beacons
#[test]
pub fn check_time_for_serialization_of_5000_beacons() {
    let now = SystemTime::now();
    let mut beacons: Vec<Beacon> = Vec::new();
    let startc = now.elapsed().unwrap().as_nanos();
    println!("Beginning creation of beacons after {}ns", startc);
    for _i in 0..5001 {
        beacons.push(rnd_beacon());
    }
    let afterc = now.elapsed().unwrap().as_nanos();
    println!("Beacon creation finished after {}ns", afterc - startc);

    let starts = now.elapsed().unwrap().as_nanos();
    println!("Starting serialization after {}ns", starts);
    for x in beacons {
        serde_cbor::to_vec(&x).expect("A problem occured while serializing");
    }
    let afters = now.elapsed().unwrap().as_nanos();

    println!("Serializing 5000 Beacons took {}ns", afters - starts);
}

/// Computes the amount of time needed to deserialize 5000 beacons
#[test]
pub fn check_time_for_deserialization_of_5000_beacons() {
    let mut beacons: Vec<Beacon> = Vec::new();
    let mut serialized: Vec<Vec<u8>> = Vec::new();

    for _i in 0..5001 {
        beacons.push(rnd_beacon());
    }

    for x in beacons {
        serialized.push(serde_cbor::to_vec(&x).expect("A problem occured while serializing"));
    }
    let now = SystemTime::now();
    let startd = now.elapsed().unwrap().as_nanos();
    println!("Starting deserialization after: {}ns", startd);
    for x in serialized {
        serde_cbor::from_slice::<Beacon>(&x).expect("Something bad happened");
    }
    let afterd = now.elapsed().unwrap().as_nanos();
    println!("Deserialization finished after: {}ns", afterd - startd);
}

/// Peer validity tests are ignored
/// They work but they make running cargo test take more time since they are time based
/// Additionally, when they are run in conjunction with the other tests sometimes they fail, probably because of some weird interactions between the test threads
/// that lead to weird behaviour in the sleeping threads
/// That said, they work flawlessly when run individually, they are just ignored out of weird behaviour in conjunction with the others and because of convenience when running the other tests
#[test]
#[ignore]
pub fn peer_validity_with_default_no_period() {
    let peer = helpers::rnd_peer();
    assert!(peer.still_valid());
    thread::sleep(Duration::from_secs(5));
    assert!(peer.still_valid());
    thread::sleep(Duration::from_secs(16));
    assert!(!peer.still_valid());
}

#[test]
#[ignore]
pub fn peer_validity_with_default_period() {
    let mut peer = helpers::rnd_peer();
    peer.period = Some(Duration::from_secs(5));
    assert!(peer.still_valid());
    thread::sleep(Duration::from_secs(5));
    assert!(peer.still_valid());
    thread::sleep(Duration::from_secs(6));
    assert!(!peer.still_valid());
}

#[test]
#[ignore]
pub fn peer_validity_with_custom() {
    let mut peer = helpers::rnd_peer();
    peer.period = Some(Duration::from_secs(5));
    (*CONFIG.lock()).custom_timeout = true;
    (*CONFIG.lock()).peer_timeout = Duration::from_secs(15);
    assert!(peer.still_valid());
    thread::sleep(Duration::from_secs(11));
    assert!(peer.still_valid());
    thread::sleep(Duration::from_secs(6));
    assert!(!peer.still_valid());
}

// Convenience functions for testing purposes

/// Generates a random beacon
pub fn rnd_beacon() -> Beacon {
    let mut rng = thread_rng();
    let rnd_duration: u8 = rng.gen_range(0..101);
    let rnd_serviceblock: u8 = rng.gen_range(0..101);
    let amount_of_services: u8 = rng.gen_range(0..11);

    let clas = [
        CLAsAvailable::MtcpConvergenceLayer,
        CLAsAvailable::HttpConvergenceLayer,
        CLAsAvailable::TcpConvergenceLayer,
        CLAsAvailable::DummyConvergenceLayer,
    ];
    let ports = [
        20, 0, 5000, 1243, 513, 1241, 324, 9441, 2435, 6234, 23, 1, 45,
    ];

    let services = if rnd_serviceblock < 50 {
        let mut buf = Vec::new();
        for _x in 0..amount_of_services {
            let rnd_scheme: usize = rng.gen_range(0..3);
            let rnd_port: usize = rng.gen_range(0..13);
            let cla = clas[rnd_scheme];
            let port = ports[rnd_port];
            let service = (cla, Some(port));
            buf.push(service);
        }
        buf
    } else {
        Vec::new()
    };
    let mut serviceblock = ServiceBlock::new();
    serviceblock.set_clas(services);

    let beacon_period = if rnd_duration < 50 {
        Some(Duration::from_secs(rng.gen_range(0..12001)))
    } else {
        None
    };

    let rnd_dtn: u8 = rng.gen_range(0..3);
    let endpoint = match rnd_dtn {
        0 => EndpointID::try_from("dtn://n1/").unwrap(),
        1 => {
            let rnd_node: u64 = rng.gen();
            let rnd_service: u64 = rng.gen();
            EndpointID::Ipn(2, IpnAddress::new(rnd_node, rnd_service))
        }
        _ => EndpointID::none(),
    };

    Beacon::with_config(endpoint, serviceblock, beacon_period)
}
