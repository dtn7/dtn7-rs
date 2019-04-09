use super::*;
use bp7::EndpointID;
use rand::distributions::Alphanumeric;
use rand::thread_rng;
use rand::Rng;
use std::net::IpAddr;

pub fn rnd_peer() -> DtnPeer {
    let peertype = match rand::thread_rng().gen_range(0, 2) {
        0 => PeerType::Static,
        _ => PeerType::Dynamic,
    };
    let rstr: String = thread_rng().sample_iter(&Alphanumeric).take(10).collect();
    let eid = EndpointID::from(format!("dtn://{}", rstr));
    match rand::thread_rng().gen_range(0, 2) {
        0 => {
            let random_bytes = rand::thread_rng().gen::<[u8; 4]>();
            DtnPeer::new(eid, IpAddr::from(random_bytes), peertype, Vec::new())
        }
        _ => {
            let random_bytes = rand::thread_rng().gen::<[u8; 16]>();
            DtnPeer::new(eid, IpAddr::from(random_bytes), peertype, Vec::new())
        }
    }
}
