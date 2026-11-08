//! PRoPHET routing, implementing the original (v1) protocol (Lindgren et al., 2003/2004).
//!
//! Each node maintains a delivery predictability `P(A,B)` in `[0,1]`
//! per known destination, updated by three rules:
//! - Encounter (Eq. 1): `P(A,B) = P(A,B)_old + (1 - P(A,B)_old) * p_init`
//! - Aging (Eq. 2): `P(A,B) = P(A,B)_old * gamma^k`,
//!   `k` being the elapsed whole time units, applied lazily on read
//! - Transitivity (Eq. 3): `P(A,C) = P(A,C)_old + (1 - P(A,C)_old) * P(A,B) * P(B,C) * beta`
//!
//! Delivery predictability maps are exchanged with encountered peers
//! as administrative bundles (see `routing::metadata`).
//! A record is unicast to the peer's node endpoint once per contact:
//! on the encounter transition, and as a reply to a map received from
//! a peer this node has not exchanged with during the current contact.
//! There is no periodic re-exchange while a contact persists.
//! RFC 6693 recommends upgrading v1 nodes
//! "because of problems that have been identified".
//! Received maps are aged with the local gamma before use.
//!
//! Forwarding uses the GRTR strategy:
//! a bundle for destination D is forwarded to peer B if `P(B,D) > P(A,D)`,
//! with a direct-delivery override when B is the destination itself.
//! GRTR is only defined for singleton destinations, so group bundles are spread epidemically.
//!
//! An "encounter" is a contact-set transition:
//! a peer counts as encountered when it appears in the peer list
//! (via discovery beacon, static peer or an incoming PRoPHET record)
//! and again only after it has left.
//! An internal tick drives this, so the algorithm also works with static peers
//! and disabled neighborhood discovery.
//! Static peers are trusted to be genuinely reachable.
//!
//! Note:
//! PRoPHET v1 targets episodic contacts.
//! In a fully static, never disrupted network each node pair exchanges maps exactly once,
//! afterwards all predictabilities decay towards zero under Eq. 2
//! and later learned information does not propagate.
//! Expect degraded behavior in such setups.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use super::{BundleHistory, ProphetRib, parse_duration, parse_probability};
use crate::cla::ClaSenderTask;
use crate::core::bundlepack::BundlePack;
use crate::routing::metadata;
use crate::routing::{RoutingAgent, RoutingCmd};
use crate::{CONFIG, PEERS, RoutingNotification, store_has_item};
use async_trait::async_trait;
use bp7::EndpointID;
use log::{debug, info, trace, warn};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

const TAG: &str = "prophet_v1";
const PROPHET_RIB_VERSION: u8 = 1;

/// Configuration for the PRoPHET v1 routing agent.
///
/// The equation parameters default to the values from the original publication
/// (p_init = 0.75, beta = 0.25, gamma = 0.98).
/// All values can be overridden via `-R prophet_v1.<key>=<value>` or `[routing.settings.prophet_v1]`.
#[derive(Debug, Clone)]
pub struct ProphetV1Config {
    /// Encounter constant of Eq. 1 (paper: 0.75).
    pub p_init: f64,
    /// Transitivity scaling constant of Eq. 3 (paper: 0.25).
    pub beta: f64,
    /// Aging constant of Eq. 2 (paper: 0.98).
    pub gamma: f64,
    /// Duration of one aging time unit K (default: 30s).
    pub time_unit: Duration,
    /// Predictabilities below this value are dropped during aging
    /// (0 disables pruning).
    pub prune_threshold: f64,
    /// Interval of the internal contact-tracking tick.
    pub tick_interval: Duration,
    /// Lifetime of the metadata bundles carrying predictability maps.
    pub record_lifetime: Duration,
}

impl Default for ProphetV1Config {
    fn default() -> Self {
        ProphetV1Config {
            p_init: 0.75,
            beta: 0.25,
            gamma: 0.98,
            time_unit: Duration::from_secs(30),
            prune_threshold: 0.0,
            tick_interval: Duration::from_secs(5),
            record_lifetime: Duration::from_secs(60),
        }
    }
}

impl ProphetV1Config {
    /// Builds a config from the `prophet_v1` section of the routing settings,
    /// panicking on invalid values so misconfiguration fails at startup.
    pub fn from_settings(settings: Option<&HashMap<String, String>>) -> ProphetV1Config {
        let mut cfg = ProphetV1Config::default();
        let empty = HashMap::new();
        let settings = settings.unwrap_or(&empty);
        cfg.p_init = parse_probability(TAG, settings, "p_init", cfg.p_init);
        cfg.beta = parse_probability(TAG, settings, "beta", cfg.beta);
        cfg.gamma = parse_probability(TAG, settings, "gamma", cfg.gamma);
        cfg.prune_threshold =
            parse_probability(TAG, settings, "prune_threshold", cfg.prune_threshold);
        cfg.time_unit = parse_duration(TAG, settings, "time_unit", cfg.time_unit);
        cfg.tick_interval = parse_duration(TAG, settings, "tick_interval", cfg.tick_interval);
        cfg.record_lifetime = parse_duration(TAG, settings, "record_lifetime", cfg.record_lifetime);
        if cfg.p_init == 0.0 || cfg.gamma == 0.0 || cfg.time_unit.is_zero() {
            panic!(
                "{}: p_init, gamma and time_unit must be greater than zero",
                TAG
            );
        }
        if cfg.tick_interval.is_zero() {
            panic!("{}: tick_interval must be greater than zero", TAG);
        }
        if cfg.record_lifetime.is_zero() {
            panic!("{}: record_lifetime must be greater than zero", TAG);
        }
        cfg
    }
}

/// A peer currently in contact.
#[derive(Debug)]
struct Contact {
    /// The peer's node endpoint.
    eid: EndpointID,
    /// Whether this node's predictability map went out to the peer during this contact
    /// (cleared again if the send fails).
    exchanged: bool,
    /// Whether this node has already processed the peer's predictability map during this contact.
    /// Additional maps are ignored because v1's additive transitivity rule is not idempotent.
    received: bool,
}

/// This node's delivery predictabilities, aged lazily on access.
struct DeliveryPredictabilities {
    preds: HashMap<String, f64>,
    last_aged: Instant,
    cfg: ProphetV1Config,
}

impl DeliveryPredictabilities {
    fn new(cfg: ProphetV1Config, now: Instant) -> DeliveryPredictabilities {
        DeliveryPredictabilities {
            preds: HashMap::new(),
            last_aged: now,
            cfg,
        }
    }

    /// Eq. 1: increases the predictability for an encountered peer.
    fn encounter(&mut self, peer: &str, now: Instant) {
        self.age(now);
        let p = self.preds.entry(peer.to_string()).or_insert(0.0);
        *p += (1.0 - *p) * self.cfg.p_init;
    }

    /// Eq. 2: multiplies all predictabilities by gamma^k,
    /// k being the number of whole time units since the last aging.
    /// `last_aged` advances by exactly k units so fractional units accumulate instead of being lost.
    fn age(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.last_aged);
        let k = (elapsed.as_secs_f64() / self.cfg.time_unit.as_secs_f64()).floor();
        if k < 1.0 {
            return;
        }
        let factor = self.cfg.gamma.powi(k as i32);
        for p in self.preds.values_mut() {
            *p *= factor;
        }
        self.last_aged += self.cfg.time_unit.mul_f64(k);
        if self.cfg.prune_threshold > 0.0 {
            self.preds.retain(|_, p| *p >= self.cfg.prune_threshold);
        }
    }

    /// Eq. 3: updates predictabilities using the map received from `peer`.
    /// Entries for this node and for `peer` itself are ignored.
    fn apply_transitivity(
        &mut self,
        peer: &str,
        received: &HashMap<String, f64>,
        local_node: &str,
        now: Instant,
    ) {
        self.age(now);
        let Some(&p_ab) = self.preds.get(peer) else {
            return;
        };
        for (dest, &p_bc) in received {
            if dest == local_node || dest == peer {
                continue;
            }
            let p = self.preds.entry(dest.clone()).or_insert(0.0);
            *p += (1.0 - *p) * p_ab * p_bc * self.cfg.beta;
        }
    }

    /// Current predictability for a destination, 0 if unknown.
    fn get(&mut self, dest: &str, now: Instant) -> f64 {
        self.age(now);
        self.preds.get(dest).copied().unwrap_or(0.0)
    }

    /// Current snapshot of all predictabilities.
    fn snapshot(&mut self, now: Instant) -> HashMap<String, f64> {
        self.age(now);
        self.preds.clone()
    }
}

#[derive(Debug)]
pub struct ProphetV1RoutingAgent {
    tx: mpsc::Sender<RoutingCmd>,
}

impl Default for ProphetV1RoutingAgent {
    fn default() -> Self {
        ProphetV1RoutingAgent::new()
    }
}

impl ProphetV1RoutingAgent {
    pub fn new() -> ProphetV1RoutingAgent {
        let settings = CONFIG.lock().routing_settings.clone();
        let cfg = ProphetV1Config::from_settings(settings.get(TAG));

        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move { handle_routing_cmd(cfg, rx).await });

        ProphetV1RoutingAgent { tx }
    }
}

impl std::fmt::Display for ProphetV1RoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ProphetV1RoutingAgent")
    }
}

#[async_trait]
impl RoutingAgent for ProphetV1RoutingAgent {
    fn channel(&self) -> Sender<RoutingCmd> {
        self.tx.clone()
    }
}

struct ProphetV1RoutingAgentCore {
    /// The configuration for this agent.
    cfg: ProphetV1Config,
    /// The local node name (from the host EID).
    local_node: String,
    /// The EID of this node.
    host_eid: EndpointID,
    /// Delivery predictabilities for this node P(A,x).
    dps: DeliveryPredictabilities,
    /// Peers currently in contact and their exchange state.
    contacts: HashMap<String, Contact>,
    /// Last received predictability map per peer: P(B,x) and time of reception.
    peer_ribs: HashMap<String, (HashMap<String, f64>, Instant)>,
    /// Metadata bundles this node created that are still in flight (bundle ID -> peer).
    metadata_bundles: HashMap<String, String>,
    /// Which bundles were already sent to which nodes.
    history: HashMap<String, BundleHistory>,
}

impl ProphetV1RoutingAgentCore {
    fn new(cfg: ProphetV1Config) -> ProphetV1RoutingAgentCore {
        let host_eid = CONFIG.lock().host_eid.clone();
        ProphetV1RoutingAgentCore::new_with_host(cfg, host_eid, Instant::now())
    }

    fn new_with_host(
        cfg: ProphetV1Config,
        host_eid: EndpointID,
        now: Instant,
    ) -> ProphetV1RoutingAgentCore {
        ProphetV1RoutingAgentCore {
            local_node: host_eid.node().expect("invalid local node EID"),
            host_eid,
            dps: DeliveryPredictabilities::new(cfg.clone(), now),
            cfg,
            contacts: HashMap::new(),
            peer_ribs: HashMap::new(),
            metadata_bundles: HashMap::new(),
            history: HashMap::new(),
        }
    }

    /// Registers a peer as in contact, applying Eq. 1 and
    /// sending it this node's predictability map if it just (re)appeared.
    /// A no-op for peers already in contact.
    fn contact_with(&mut self, peer_name: &str, eid: &EndpointID, now: Instant) {
        if peer_name == self.local_node || self.contacts.contains_key(peer_name) {
            return;
        }
        debug!("{}: encountered peer {}", TAG, peer_name);
        self.contacts.insert(
            peer_name.to_string(),
            Contact {
                eid: eid.clone(),
                exchanged: false,
                received: false,
            },
        );
        self.dps.encounter(peer_name, now);
        self.send_rib(peer_name, now);
    }

    /// Removes a peer from the contacts map.
    fn contact_lost(&mut self, peer_name: &str) {
        if self.contacts.remove(peer_name).is_some() {
            debug!("{}: lost contact with peer {}", TAG, peer_name);
        }
    }

    /// Updates the exchange flag of a contact, if the peer is still one.
    fn set_exchanged(&mut self, peer_name: &str, exchanged: bool) {
        if let Some(contact) = self.contacts.get_mut(peer_name) {
            contact.exchanged = exchanged;
        }
    }

    /// Sends this node's predictability map to a peer in contact.
    /// This happens once per contact:
    /// on the encounter transition and as a reply to a map from a peer this node has not exchanged with yet.
    fn send_rib(&mut self, peer_name: &str, now: Instant) {
        let Some(peer_eid) = self.contacts.get(peer_name).map(|c| c.eid.clone()) else {
            return;
        };

        let rib = ProphetRib {
            version: PROPHET_RIB_VERSION,
            node: self.local_node.clone(),
            preds: self.dps.snapshot(now),
        };
        match serde_cbor::to_vec(&rib) {
            Ok(data) => {
                let bndl = metadata::new_routing_record_bundle(
                    &self.host_eid,
                    &peer_eid,
                    metadata::PROPHET_RIB_RECORD,
                    data,
                    self.cfg.record_lifetime,
                );
                debug!(
                    "{}: sending predictability map to {} ({} entries)",
                    TAG,
                    peer_name,
                    rib.preds.len()
                );
                let bid = bndl.id();
                self.metadata_bundles
                    .insert(bid.clone(), peer_name.to_string());
                self.set_exchanged(peer_name, true);
                // The record reaches the store before this returns,
                // so `prune_gone_bundles` can never see it as gone while it is still tracked here.
                if let Err(err) = metadata::send_routing_record_bundle(bndl) {
                    warn!(
                        "{}: could not send predictability map to {}: {}",
                        TAG, peer_name, err
                    );
                    // Nothing was sent, so no transfer result will ever arrive for it.
                    self.metadata_bundles.remove(&bid);
                    self.set_exchanged(peer_name, false);
                }
            }
            Err(err) => warn!("{}: could not encode predictability map: {}", TAG, err),
        }
    }

    /// Contact tracking tick: diffs the global peer list against the contact set.
    /// This also covers static peers and disabled neighborhood discovery,
    /// where no `EncounteredPeer` notifications are emitted.
    fn handle_tick(&mut self, now: Instant) {
        let peers: Vec<(String, EndpointID)> = PEERS
            .lock()
            .iter()
            .map(|(name, p)| (name.clone(), p.eid.clone()))
            .collect();
        let current: HashSet<&String> = peers.iter().map(|(name, _)| name).collect();
        let gone: Vec<String> = self
            .contacts
            .keys()
            .filter(|name| !current.contains(name))
            .cloned()
            .collect();
        for name in gone {
            self.contact_lost(&name);
        }
        for (name, eid) in &peers {
            self.contact_with(name, eid, now);
        }
    }

    /// Drops tracking state for bundles that can no longer come back.
    fn prune_gone_bundles(&mut self, now: Instant) {
        // Forwarding history must outlive the stored bundle.
        self.history
            .retain(|bid, history| now < history.expires || store_has_item(bid));
        let gone: Vec<String> = self
            .metadata_bundles
            .keys()
            .filter(|bid| !store_has_item(bid))
            .cloned()
            .collect();
        for bid in gone {
            if let Some(peer_name) = self.metadata_bundles.remove(&bid) {
                // The record expired or was discarded without a send result.
                // Permit one later inbound map to trigger a fresh reply.
                self.set_exchanged(&peer_name, false);
            }
        }
    }

    /// Handles a received predictability map from `src`.
    fn handle_rib(&mut self, src: &EndpointID, data: &[u8], now: Instant) {
        let Some(name) = src.node() else {
            warn!("{}: received record without source node: {}", TAG, src);
            return;
        };
        if name == self.local_node {
            warn!("{}: ignoring record sourced from the local node", TAG);
            return;
        }
        let rib: ProphetRib = match serde_cbor::from_slice(data) {
            Ok(rib) => rib,
            Err(err) => {
                warn!("{}: could not decode record from {}: {}", TAG, name, err);
                return;
            }
        };
        if rib.version != PROPHET_RIB_VERSION {
            warn!(
                "{}: unsupported record version {} from {}",
                TAG, rib.version, name
            );
            return;
        }
        if !rib.is_valid() {
            warn!(
                "{}: record from {} contains invalid destination names or predictabilities, ignoring",
                TAG, name
            );
            return;
        }
        if rib.node != name {
            warn!(
                "{}: record node {} does not match source {}, trusting source",
                TAG, rib.node, name
            );
        }
        debug!(
            "{}: received predictability map from {} ({} entries)",
            TAG,
            name,
            rib.preds.len()
        );

        // Receiving a record implies contact, even if this node has not noticed the peer itself yet.
        self.contact_with(&name, src, now);
        let Some(contact) = self.contacts.get_mut(&name) else {
            return;
        };
        if contact.received {
            let needs_reply = !contact.exchanged;
            debug!(
                "{}: ignoring additional predictability map from {} during the same contact",
                TAG, name
            );
            if needs_reply {
                // The map is not applied again,
                // but it can recover the other half of the exchange
                // if this node's earlier record failed or expired.
                self.send_rib(&name, now);
            }
            return;
        }
        contact.received = true;
        self.dps
            .apply_transitivity(&name, &rib.preds, &self.local_node.clone(), now);
        self.peer_ribs.insert(name.clone(), (rib.preds, now));
        // One exchange per contact:
        // reply if this node's map has not reached the peer yet,
        // e.g. because it was down when it was greeted.
        if self
            .contacts
            .get(&name)
            .is_some_and(|contact| !contact.exchanged)
        {
            self.send_rib(&name, now);
        }
    }

    /// Predictability of `peer_name` for `dest` from its last received map,
    /// aged with this node's own gamma since reception (approximation).
    fn peer_pred(&self, peer_name: &str, dest: &str, now: Instant) -> f64 {
        let Some((preds, received_at)) = self.peer_ribs.get(peer_name) else {
            return 0.0;
        };
        let Some(&p) = preds.get(dest) else {
            return 0.0;
        };
        let elapsed = now.saturating_duration_since(*received_at);
        let k = (elapsed.as_secs_f64() / self.cfg.time_unit.as_secs_f64()).floor();
        if k < 1.0 {
            p
        } else {
            p * self.cfg.gamma.powi(k as i32)
        }
    }

    /// Handles a routing notification.
    fn handle_notification(&mut self, notification: RoutingNotification) {
        let now = Instant::now();
        match notification {
            RoutingNotification::EncounteredPeer(eid) => {
                if let Some(name) = eid.node() {
                    self.contact_with(&name, &eid, now);
                }
            }
            RoutingNotification::DroppedPeer(eid) => {
                if let Some(name) = eid.node() {
                    self.contact_lost(&name);
                }
            }
            RoutingNotification::RoutingRecord {
                record_type,
                source,
                data,
            } => {
                if record_type == metadata::PROPHET_RIB_RECORD {
                    self.handle_rib(&source, &data, now);
                } else {
                    debug!("{}: ignoring routing record of type {}", TAG, record_type);
                }
            }
            RoutingNotification::SendingFailed(bid, node_name) => {
                if let Some(peer_name) = self.metadata_bundles.remove(&bid) {
                    if let Err(err) = crate::store_remove(&bid) {
                        warn!(
                            "{}: could not remove failed predictability map {}: {}",
                            TAG, bid, err
                        );
                    }
                    // A record received later proves bidirectional reachability
                    // and may trigger one fresh reply during this contact.
                    self.set_exchanged(&peer_name, false);
                    debug!(
                        "{}: sending predictability map {} to {} failed",
                        TAG, bid, node_name
                    );
                } else if let Some(history) = self.history.get_mut(&bid) {
                    // Allow a retry towards this peer.
                    history.nodes.remove(&node_name);
                }
            }
            RoutingNotification::SendingSucceeded(bid, node_name) => {
                if self.metadata_bundles.remove(&bid).is_some() {
                    // Confirm the exchange in case another path cleared the flag
                    // while the transfer was in progress.
                    self.set_exchanged(&node_name, true);
                }
            }
            RoutingNotification::IncomingBundle(bndl) => {
                if let Some(eid) = bndl.previous_node()
                    && let Some(node_name) = eid.node()
                {
                    self.history
                        .entry(bndl.id())
                        .or_insert_with(|| BundleHistory::new(now + bndl.primary.lifetime))
                        .nodes
                        .insert(node_name);
                };
            }
            RoutingNotification::IncomingBundleWithoutPreviousNode(bid, node_name) => {
                // This notification carries no bundle, so take the lifetime from the store.
                let lifetime = crate::store_get_metadata(&bid)
                    .map_or(Duration::ZERO, |meta| Duration::from_millis(meta.lifetime));
                self.history
                    .entry(bid)
                    .or_insert_with(|| BundleHistory::new(now + lifetime))
                    .nodes
                    .insert(node_name);
            }
        }
    }

    /// Returns the list of CLA senders to which a bundle should be sent,
    /// and whether the bundle can be deleted afterwards
    /// (direct delivery to its destination, or one of this node's own metadata bundles).
    fn senders_for_bundle(&mut self, bp: &BundlePack) -> (Vec<ClaSenderTask>, bool) {
        let now = Instant::now();

        // This node's own metadata bundles only ever go directly to their target peer.
        if let Some(peer_name) = self.metadata_bundles.get(bp.id()) {
            let cla = crate::peers_get_for_node(&bp.destination).and_then(|p| p.first_cla());
            return match cla {
                Some(cla) => (vec![cla], true),
                None => {
                    // Without senders, `forward` deletes the bundle right away.
                    debug!(
                        "{}: dropping predictability map {} for vanished peer {}",
                        TAG,
                        bp.id(),
                        peer_name
                    );
                    (vec![], true)
                }
            };
        }

        let Some(dest) = bp.destination.node() else {
            return (vec![], false);
        };

        let is_group = bp.destination.is_non_singleton();
        let p_local = self.dps.get(&dest, now);
        let mut history = self
            .history
            .remove(bp.id())
            .unwrap_or_else(|| BundleHistory::new(now + Duration::from_millis(bp.lifetime)));

        let mut selected: Vec<(String, ClaSenderTask)> = Vec::new();
        let mut delete_afterwards = false;
        for peer in PEERS.lock().values() {
            let peer_name = peer.node_name();
            if peer_name == self.local_node || history.nodes.contains(&peer_name) {
                continue;
            }

            // Spread group bundles epidemically.
            if is_group {
                if let Some(cla) = peer.first_cla() {
                    debug!(
                        "{}: relaying group bundle {} to {}",
                        TAG,
                        bp.id(),
                        peer_name
                    );
                    selected.push((peer_name, cla));
                }
                continue;
            }

            // Direct delivery to the destination supersedes any relay candidates.
            if peer_name == dest {
                if let Some(cla) = peer.first_cla() {
                    debug!(
                        "{}: attempting direct delivery of bundle {} to {}",
                        TAG,
                        bp.id(),
                        peer_name
                    );
                    delete_afterwards = true;
                    selected.clear();
                    selected.push((peer_name, cla));
                    break;
                }
                continue;
            }

            let p_peer = self.peer_pred(&peer_name, &dest, now);
            trace!(
                "{}: GRTR for {} via {}: P(peer,dest)={} vs P(local,dest)={}",
                TAG,
                bp.id(),
                peer_name,
                p_peer,
                p_local
            );
            if p_peer > p_local
                && let Some(cla) = peer.first_cla()
            {
                debug!("{}: relaying bundle {} to {}", TAG, bp.id(), peer_name);
                selected.push((peer_name, cla));
            }
        }

        let clas = selected
            .into_iter()
            .map(|(name, cla)| {
                history.nodes.insert(name);
                cla
            })
            .collect();
        self.history.insert(bp.id().to_string(), history);
        (clas, delete_afterwards)
    }

    /// Returns a JSON representation of the current state of the routing agent.
    fn as_json(&mut self) -> String {
        let now = Instant::now();
        let peer_ribs: serde_json::Map<String, serde_json::Value> = self
            .peer_ribs
            .iter()
            .map(|(name, (preds, received_at))| {
                (
                    name.clone(),
                    serde_json::json!({
                        "age_secs": now.saturating_duration_since(*received_at).as_secs(),
                        "preds": preds,
                    }),
                )
            })
            .collect();
        let contacts: serde_json::Map<String, serde_json::Value> = self
            .contacts
            .iter()
            .map(|(name, contact)| {
                (
                    name.clone(),
                    serde_json::json!({
                        "exchanged": contact.exchanged,
                        "received": contact.received,
                    }),
                )
            })
            .collect();
        serde_json::json!({
            "config": {
                "p_init": self.cfg.p_init,
                "beta": self.cfg.beta,
                "gamma": self.cfg.gamma,
                "time_unit_secs": self.cfg.time_unit.as_secs_f64(),
                "prune_threshold": self.cfg.prune_threshold,
            },
            "local": self.dps.snapshot(now),
            "peer_ribs": peer_ribs,
            "contacts": contacts,
        })
        .to_string()
    }
}

/// The main loop of the PRoPHET routing agent, running in its own task.
async fn handle_routing_cmd(cfg: ProphetV1Config, mut rx: mpsc::Receiver<RoutingCmd>) {
    info!("{}: starting with {:?}", TAG, cfg);

    let mut core = ProphetV1RoutingAgentCore::new(cfg);
    let mut tick = tokio::time::interval(core.cfg.tick_interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut tick_count: u64 = 0;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                core.handle_tick(Instant::now());
                tick_count += 1;
                // Pruning queries the store once per tracked bundle,
                // so run it far less often than the contact tick
                // (every 12th tick = once a minute at the default 5s interval).
                if tick_count.is_multiple_of(12) {
                    core.prune_gone_bundles(Instant::now());
                }
            }
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { break; };
                match cmd {
                    RoutingCmd::SenderForBundle(bp, reply) => {
                        let senders = core.senders_for_bundle(&bp);
                        tokio::spawn(async move {
                            // the requester may already be gone (e.g., timed out), which is not an error here
                            let _ = reply.send(senders);
                        });
                    }
                    RoutingCmd::Shutdown => break,
                    RoutingCmd::Command(_) => {}
                    RoutingCmd::GetData(_, tx) => {
                        // the requester may already be gone (e.g., timed out), which is not an error here
                        let _ = tx.send(core.as_json());
                    }
                    RoutingCmd::Notify(notification) => core.handle_notification(notification),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{} != {}", a, b);
    }

    #[test]
    fn encounter_first_and_repeated() {
        let cfg = ProphetV1Config::default();
        let t0 = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg, t0);
        dps.encounter("b", t0);
        assert_close(dps.get("b", t0), 0.75);
        dps.encounter("b", t0);
        assert_close(dps.get("b", t0), 0.75 + (1.0 - 0.75) * 0.75);
    }

    #[test]
    fn aging_applies_gamma_per_whole_time_unit() {
        let cfg = ProphetV1Config::default();
        let t0 = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg.clone(), t0);
        dps.encounter("b", t0);
        let aged = dps.get("b", t0 + cfg.time_unit.mul_f64(2.5));
        assert_close(aged, 0.75 * cfg.gamma * cfg.gamma);
    }

    #[test]
    fn aging_preserves_fractional_time_units() {
        let cfg = ProphetV1Config::default();
        let t0 = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg.clone(), t0);
        dps.encounter("b", t0);

        // Two agings of half a unit each must apply gamma exactly once.
        dps.age(t0 + cfg.time_unit.mul_f64(0.5));
        assert_close(dps.get("b", t0 + cfg.time_unit.mul_f64(0.5)), 0.75);
        assert_close(dps.get("b", t0 + cfg.time_unit), 0.75 * cfg.gamma);
    }

    #[test]
    fn aging_prunes_low_predictabilities() {
        let cfg = ProphetV1Config {
            prune_threshold: 0.5,
            ..Default::default()
        };
        let t0 = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg.clone(), t0);
        dps.encounter("b", t0);

        // 0.75 * 0.98^21 < 0.5
        let later = t0 + cfg.time_unit * 21;
        assert_close(dps.get("b", later), 0.0);
        assert!(dps.snapshot(later).is_empty());

        // A subsequent encounter starts from scratch again.
        dps.encounter("b", later);
        assert_close(dps.get("b", later), 0.75);
    }

    #[test]
    fn transitivity_is_additive_and_skips_self_and_peer() {
        let cfg = ProphetV1Config::default();
        let t0 = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg.clone(), t0);
        dps.encounter("b", t0);
        let received: HashMap<String, f64> = [
            ("c".to_string(), 0.75),
            ("a".to_string(), 0.9),
            ("b".to_string(), 0.5),
        ]
        .into();
        dps.apply_transitivity("b", &received, "a", t0);

        // P(a,c) = 0 + (1 - 0) * P(a,b) * P(b,c) * beta
        let p1 = 0.75 * 0.75 * 0.25;
        assert_close(dps.get("c", t0), p1);
        // no self entry, P(a,b) untouched
        assert_close(dps.get("a", t0), 0.0);
        assert_close(dps.get("b", t0), 0.75);
        // a second application accumulates onto the old value
        let weaker: HashMap<String, f64> = [("c".to_string(), 0.1)].into();
        dps.apply_transitivity("b", &weaker, "a", t0);
        assert_close(dps.get("c", t0), p1 + (1.0 - p1) * 0.75 * 0.1 * 0.25);
    }

    #[test]
    fn transitivity_without_peer_predictability_is_noop() {
        let cfg = ProphetV1Config::default();
        let t0 = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg, t0);
        let received: HashMap<String, f64> = [("c".to_string(), 0.75)].into();
        dps.apply_transitivity("b", &received, "a", t0);
        assert_close(dps.get("c", t0), 0.0);
    }

    #[test]
    fn config_defaults_and_overrides() {
        let cfg = ProphetV1Config::from_settings(None);
        assert_close(cfg.p_init, 0.75);
        assert_close(cfg.beta, 0.25);
        assert_close(cfg.gamma, 0.98);
        assert_eq!(cfg.time_unit, Duration::from_secs(30));

        let settings: HashMap<String, String> = [
            ("p_init".to_string(), "0.5".to_string()),
            ("time_unit".to_string(), "10s".to_string()),
        ]
        .into();
        let cfg = ProphetV1Config::from_settings(Some(&settings));
        assert_close(cfg.p_init, 0.5);
        assert_eq!(cfg.time_unit, Duration::from_secs(10));
        assert_close(cfg.beta, 0.25);
    }

    #[test]
    #[should_panic(expected = "must be within [0,1]")]
    fn config_rejects_invalid_probability() {
        let settings: HashMap<String, String> = [("gamma".to_string(), "1.5".to_string())].into();
        ProphetV1Config::from_settings(Some(&settings));
    }

    #[test]
    #[should_panic(expected = "tick_interval must be greater than zero")]
    fn config_rejects_zero_tick_interval() {
        let settings: HashMap<String, String> =
            [("tick_interval".to_string(), "0s".to_string())].into();
        ProphetV1Config::from_settings(Some(&settings));
    }

    #[test]
    #[should_panic(expected = "record_lifetime must be greater than zero")]
    fn config_rejects_zero_record_lifetime() {
        let settings: HashMap<String, String> =
            [("record_lifetime".to_string(), "0s".to_string())].into();
        ProphetV1Config::from_settings(Some(&settings));
    }

    #[test]
    fn delivered_bundle_history_survives_until_the_bundle_expires() {
        let now = Instant::now();
        let life = Duration::from_secs(60);
        let mut core = ProphetV1RoutingAgentCore::new_with_host(
            ProphetV1Config::default(),
            "dtn://a/".try_into().unwrap(),
            now,
        );
        let mut history = BundleHistory::new(now + life);
        history.nodes.insert("b".into());
        core.history.insert("bundle".into(), history);

        // The bundle survives until its lifetime expires, even if it is no longer in the store.
        core.prune_gone_bundles(now + Duration::from_secs(30));
        assert!(core.history["bundle"].nodes.contains("b"));

        // Once it can no longer be received again, the history is dropped.
        core.prune_gone_bundles(now + Duration::from_secs(61));
        assert!(!core.history.contains_key("bundle"));
    }
}
