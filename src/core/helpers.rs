use super::*;
use bp7::EndpointID;
use rand::distributions::Alphanumeric;
use rand::thread_rng;
use rand::Rng;
use std::net::IpAddr;
use url::Url;

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

/// # Example
///
/// ```
/// use dtn7::core::helpers::parse_peer_url;
/// use bp7::EndpointID;
///
/// let peer = parse_peer_url("stcp://192.168.2.1/node1");
/// assert_eq!(peer.eid, EndpointID::from("dtn://node1".to_string()));
/// ```
///
/// An invalid convergency layer should panic:
/// ```should_panic
/// use dtn7::core::helpers::parse_peer_url;
///
/// parse_peer_url("nosuchcla://192.168.2.1/node1");
/// ```
///
/// A missing nodeid should also trigger a panic:
/// ```should_panic
/// use dtn7::core::helpers::parse_peer_url;
///
/// parse_peer_url("stcp://192.168.2.1");
/// ```
pub fn parse_peer_url(peer_url: &str) -> DtnPeer {
    let u = Url::parse(&peer_url).expect("Static peer url parsing error");
    let scheme = u.scheme();
    if !crate::cla::convergency_layer_agents().contains(&scheme) {
        panic!("Unknown convergency layer selected: {}", scheme);
    }
    let ipaddr = u.host_str().expect("Host parsing error");
    let nodeid = u.path();
    if nodeid == "/" {
        panic!("Missing node id");
    }
    DtnPeer::new(
        format!("dtn://{}", nodeid.replace('/', "")).into(),
        ipaddr.parse().unwrap(),
        PeerType::Static,
        vec![scheme.into()],
    )
}
