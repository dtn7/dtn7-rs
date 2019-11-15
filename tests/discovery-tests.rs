use dtn7::{PEERS, peers_add, peers_get_for_node, peers_count, peers_clear};
use dtn7::core::helpers::rnd_peer;

#[test]
fn discovery_store_test() {
    peers_clear();
    println!("discovery store test");
    assert_eq!(peers_count(), 0);
    let peer = rnd_peer();
    peers_add(peer);
    assert_eq!(peers_count(), 1);

    let peer = rnd_peer();
    peers_add(peer);
    assert_eq!(peers_count(), 2);

    peers_clear(); // unclear why this is needed for the next test to pass..
}

#[test]
fn discovery_store_last_contact_test() {
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

    assert_ne!(first_contact, peers_get_for_node(&peer.eid).unwrap().last_contact);

}