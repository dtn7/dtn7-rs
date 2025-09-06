use crate::cla::CLAsAvailable;
use crate::core::peer::PeerAddress;

use super::*;
use bp7::EndpointID;
use rand::distr::Alphanumeric;
use rand::Rng;
use sha1::{Digest, Sha1};
use std::{convert::TryFrom, net::IpAddr};
use thiserror::Error;
use url::Url;

pub fn rnd_peer() -> DtnPeer {
    let peertype = match rand::rng().random_range(0..2) {
        0 => PeerType::Static,
        _ => PeerType::Dynamic,
    };
    let rstr: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let eid = EndpointID::try_from(format!("dtn://{}", rstr)).unwrap();
    match rand::rng().random_range(0..2) {
        0 => {
            let random_bytes = rand::rng().random::<[u8; 4]>();
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
            let random_bytes = rand::rng().random::<[u8; 16]>();
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

/// Peer Connection URL Parsing Errors
#[derive(Error, Debug, PartialEq, Eq)]
pub enum ParsePeerUrlError {
    #[error("invalid URL format error")]
    InvalidUrl,
    #[error("invalid nodeid error")]
    InvalidNodeId,
    #[error("no such CLA registered error")]
    NoSuchCLA(String),
    #[error("unknown peer URL parsing error")]
    Unknown,
}

/// # Example
///
/// ```
/// use std::convert::TryFrom;
/// use dtn7::core::helpers::parse_peer_url;
/// use bp7::EndpointID;
///
/// let peer = parse_peer_url("mtcp://192.168.2.1:2342/node1").unwrap();
/// assert_eq!(peer.eid, EndpointID::try_from("dtn://node1".to_string()).unwrap());
/// ```
///
/// An invalid convergency layer should panic:
/// ```should_panic
/// use dtn7::core::helpers::parse_peer_url;
///
/// parse_peer_url("nosuchcla://192.168.2.1/node1").unwrap();
/// ```
///
/// A missing nodeid should also trigger a panic:
/// ```should_panic
/// use dtn7::core::helpers::parse_peer_url;
///
/// parse_peer_url("mtcp://192.168.2.1").unwrap();
/// ```
pub fn parse_peer_url(peer_url: &str) -> Result<DtnPeer, ParsePeerUrlError> {
    let u: Url;
    let is_external = if peer_url.starts_with("ecla+") {
        u = if let Ok(parsed_url) = Url::parse(peer_url.strip_prefix("ecla+").unwrap()) {
            parsed_url
        } else {
            return Err(ParsePeerUrlError::InvalidUrl);
        };

        true
    } else {
        u = if let Ok(parsed_url) = Url::parse(peer_url) {
            parsed_url
        } else {
            return Err(ParsePeerUrlError::InvalidUrl);
        };

        false
    };

    let scheme = u.scheme();
    if !is_external && scheme.parse::<CLAsAvailable>().is_err() {
        return Err(ParsePeerUrlError::NoSuchCLA(scheme.into()));
    }
    let ipaddr = if let Some(host_part) = u.host_str() {
        host_part
    } else {
        return Err(ParsePeerUrlError::InvalidUrl);
    };
    let port = u.port();

    /*let cla_target: String = if port.is_some() {
        format!("{}:{}", scheme, port.unwrap())
    } else {
        scheme.into()
    };*/
    let nodeid = u.path();
    if nodeid == "/" || nodeid.is_empty() {
        return Err(ParsePeerUrlError::InvalidNodeId);
    }

    let addr = if let Ok(ip) = ipaddr.parse::<IpAddr>() {
        PeerAddress::Ip(ip)
    } else {
        PeerAddress::Generic(ipaddr.to_owned())
    };
    let nodeid = nodeid.replace('/', "");
    let eid_str = if nodeid.chars().all(char::is_numeric) {
        format!("ipn:{}.0", nodeid)
    } else {
        format!("dtn://{}/", nodeid)
    };

    if let Ok(eid) = EndpointID::try_from(eid_str) {
        Ok(DtnPeer::new(
            eid,
            addr,
            PeerType::Static,
            None,
            vec![(scheme.into(), port)],
            HashMap::new(),
        ))
    } else {
        Err(ParsePeerUrlError::Unknown)
    }
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
    let mut hasher = Sha1::new();
    for bid in bids {
        hasher.update(bid.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}
