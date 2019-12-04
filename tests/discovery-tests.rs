use dtn7::core::helpers::rnd_peer;
use dtn7::{peers_add, peers_clear, peers_count, peers_get_for_node, PEERS};
use lazy_static::*;
use std::sync::{Arc, Mutex};

lazy_static! {
    pub static ref GLOBAL_ACCESS: Arc<Mutex<u32>> = Arc::new(Mutex::new(0));
}
#[test]
fn discovery_store_test() {
    let mut count = GLOBAL_ACCESS.lock().unwrap();
    println!("call clear");
    peers_clear();
    println!("discovery store test");
    assert_eq!(peers_count(), 0);
    let peer = rnd_peer();
    peers_add(peer);
    assert_eq!(peers_count(), 1);

    let peer = rnd_peer();
    peers_add(peer);
    assert_eq!(peers_count(), 2);

    println!("call clear");
    peers_clear(); // unclear why this is needed for the next test to pass..
    *count += 1;
}

#[test]
fn discovery_store_last_contact_test() {
    let mut count = GLOBAL_ACCESS.lock().unwrap();
    println!("call clear");
    peers_clear();
    println!("discovery store last contact test");
    assert_eq!(peers_count(), 0);

    let mut peer = rnd_peer();
    let first_contact = peer.last_contact;
    peers_add(peer.clone());
    assert_eq!(peers_count(), 1);

    peer.last_contact += 1;
    peers_add(peer.clone());
    assert_eq!(peers_count(), 1);

    assert_ne!(
        first_contact,
        peers_get_for_node(&peer.eid).unwrap().last_contact
    );
    println!("call clear");
    peers_clear();
    *count += 1;
}
