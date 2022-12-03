use crate::cla::CLAsAvailable;
use crate::core::peer::PeerAddress;

use super::*;
use bp7::EndpointID;
use rand::distributions::Alphanumeric;
use rand::thread_rng;
use rand::Rng;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::{
    convert::{TryFrom, TryInto},
    net::IpAddr,
};
use url::Url;

pub fn rnd_peer() -> DtnPeer {
    let peertype = match rand::thread_rng().gen_range(0..2) {
        0 => PeerType::Static,
        _ => PeerType::Dynamic,
    };
    let rstr: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let eid = EndpointID::try_from(format!("dtn://{}", rstr)).unwrap();
    match rand::thread_rng().gen_range(0..2) {
        0 => {
            let random_bytes = rand::thread_rng().gen::<[u8; 4]>();
            DtnPeer::new(
                eid,
                IpAddr::from(random_bytes).into(),
                peertype,
                None,
                Vec::new(),
                HashMap::new(),
            )
        }
        _ => {
            let random_bytes = rand::thread_rng().gen::<[u8; 16]>();
            DtnPeer::new(
                eid,
                IpAddr::from(random_bytes).into(),
                peertype,
                None,
                Vec::new(),
                HashMap::new(),
            )
        }
    }
}

/// # Example
///
/// ```
/// use std::convert::TryFrom;
/// use dtn7::core::helpers::parse_peer_url;
/// use bp7::EndpointID;
///
/// let peer = parse_peer_url("mtcp://192.168.2.1:2342/node1");
/// assert_eq!(peer.eid, EndpointID::try_from("dtn://node1".to_string()).unwrap());
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
/// parse_peer_url("mtcp://192.168.2.1");
/// ```
pub fn parse_peer_url(peer_url: &str) -> DtnPeer {
    let u: Url;
    let is_external = if peer_url.starts_with("ecla+") {
        u = Url::parse(peer_url.strip_prefix("ecla+").unwrap())
            .expect("Static external peer url parsing error");

        true
    } else {
        u = Url::parse(peer_url).expect("Static peer url parsing error");

        false
    };

    let scheme = u.scheme();
    if !is_external && scheme.parse::<CLAsAvailable>().is_err() {
        panic!("Unknown convergency layer selected: {}", scheme);
    }
    let ipaddr = u.host_str().expect("Host parsing error");
    let port = u.port();

    /*let cla_target: String = if port.is_some() {
        format!("{}:{}", scheme, port.unwrap())
    } else {
        scheme.into()
    };*/
    let nodeid = u.path();
    if nodeid == "/" || nodeid.is_empty() {
        panic!("Missing node id");
    }

    let addr = if let Ok(ip) = ipaddr.parse::<IpAddr>() {
        PeerAddress::Ip(ip)
    } else {
        PeerAddress::Generic(ipaddr.to_owned())
    };

    DtnPeer::new(
        format!("dtn://{}/", nodeid.replace('/', ""))
            .try_into()
            .unwrap(),
        addr,
        PeerType::Static,
        None,
        vec![(scheme.into(), port)],
        HashMap::new(),
    )
}

/// check node names for validity
/// pattern similar to hostnames
/// - must start with a letter (or all digits for IPN)
/// - must contain only letters, digits, and .-_
/// - must end with a letter or digit
pub fn is_valid_node_name(name: &str) -> bool {
    let mut chars = name.chars();
    let valid_dtn = chars.next().unwrap().is_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && name.chars().last().unwrap().is_ascii_alphanumeric();
    let valid_ipn = name.chars().all(|c| c.is_ascii_digit());

    valid_dtn || valid_ipn
}

// TODO: check in more detail with ~ and / positions
pub fn is_valid_service_name(name: &str) -> bool {
    name.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.' || c == '~'
    })
}

pub fn get_complete_digest() -> String {
    let mut bids: Vec<String> = (*STORE.lock())
        .bundles()
        .iter()
        //.filter(|bp| !bp.has_constraint(Constraint::Deleted)) // deleted bundles were once known, thus, we don't need them again
        .map(|bp| bp.id.to_string())
        .collect();
    bids.sort();
    get_digest_of_bids(&bids)
}

pub fn get_digest_of_bids(bids: &[String]) -> String {
    let mut hasher = DefaultHasher::new();
    for bid in bids {
        hasher.write(bid.as_bytes());
    }
    format!("{:x}", hasher.finish())
}
