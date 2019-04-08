use rand::Rng;
use std::net::IpAddr;
use crate::core::*;


pub fn rnd_peer() -> core::DtnPeer {
    let peertype = match rand::thread_rng().gen_range(0, 2) {
        0 => core::PeerType::Static,
        _ => core::PeerType::Dynamic,
    };
    match rand::thread_rng().gen_range(0, 2) {
        0 => {
            let random_bytes = rand::thread_rng().gen::<[u8; 4]>();
            core::DtnPeer::new(None, IpAddr::from(random_bytes), peertype, Vec::new())
        }
        _ => {
            let random_bytes = rand::thread_rng().gen::<[u8; 16]>();
            core::DtnPeer::new(None, IpAddr::from(random_bytes), peertype, Vec::new())
        }
    }
}
