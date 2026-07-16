//! Shared logic for the PRoPHET routing agents.

pub mod v1;
pub mod v2;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::core::helpers::is_valid_node_name;

/// Peers a bundle was already forwarded to.
///
/// The history outlives the stored bundle.
/// A singleton bundle is deleted after direct delivery,
/// which also drops it from the store's duplicate check,
/// so a copy still held by another relay can come back.
/// Forgetting the history at that point would forward it again to peers that already have it.
/// Entries are therefore kept until the bundle can no longer be received again.
#[derive(Debug)]
struct BundleHistory {
    /// Peers this node already sent the bundle to.
    nodes: HashSet<String>,
    /// Point in time after which the bundle can no longer be received again.
    expires: Instant,
}

impl BundleHistory {
    fn new(expires: Instant) -> BundleHistory {
        BundleHistory {
            nodes: HashSet::new(),
            expires,
        }
    }
}

/// Parses a probability setting in the inclusive range [0, 1].
fn parse_probability(
    tag: &str,
    settings: &HashMap<String, String>,
    key: &str,
    default: f64,
) -> f64 {
    settings.get(key).map_or(default, |value| {
        let probability = value
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("{}: invalid value for {}: {}", tag, key, value));
        if !(0.0..=1.0).contains(&probability) {
            panic!("{}: {} must be within [0,1], got {}", tag, key, probability);
        }
        probability
    })
}

/// Parses a human-readable duration setting.
fn parse_duration(
    tag: &str,
    settings: &HashMap<String, String>,
    key: &str,
    default: Duration,
) -> Duration {
    settings.get(key).map_or(default, |value| {
        humantime::parse_duration(value)
            .unwrap_or_else(|_| panic!("{}: invalid duration for {}: {}", tag, key, value))
    })
}

/// Delivery predictability map carried in a PRoPHET routing record
/// (`metadata::PROPHET_RIB_RECORD`), keyed by node name (`EndpointID::node()`).
///
/// This representation is specific to dtn7-rs, which uses administrative
/// records rather than the TLV exchange specified in RFC 6693.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProphetRib {
    version: u8,
    node: String,
    preds: HashMap<String, f64>,
}

impl ProphetRib {
    /// Checks that all destination keys are valid node names and
    /// all predictabilities are within [0, 1], which also rules out NaN and infinities.
    fn is_valid(&self) -> bool {
        self.preds.iter().all(|(node, probability)| {
            !node.is_empty() && is_valid_node_name(node) && (0.0..=1.0).contains(probability)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rib(node: &str, probability: f64) -> ProphetRib {
        ProphetRib {
            version: 1,
            node: "node1".to_string(),
            preds: [(node.to_string(), probability)].into(),
        }
    }

    #[test]
    fn rib_cbor_roundtrip() {
        let data = serde_cbor::to_vec(&rib("node2", 0.75)).unwrap();
        let parsed: ProphetRib = serde_cbor::from_slice(&data).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.node, "node1");
        assert_eq!(parsed.preds["node2"], 0.75);
    }

    #[test]
    fn rib_rejects_invalid_predictabilities() {
        assert!(rib("node2", 0.0).is_valid());
        assert!(rib("node2", 1.0).is_valid());
        assert!(!rib("node2", f64::NAN).is_valid());
        assert!(!rib("node2", f64::INFINITY).is_valid());
        assert!(!rib("node2", f64::NEG_INFINITY).is_valid());
        assert!(!rib("node2", 1.5).is_valid());
        assert!(!rib("node2", -0.1).is_valid());
    }

    #[test]
    fn rib_rejects_invalid_destination_names() {
        assert!(rib("node2", 0.5).is_valid());
        assert!(rib("42", 0.5).is_valid());
        assert!(!rib("", 0.5).is_valid());
        assert!(!rib("node2/", 0.5).is_valid());
    }
}
