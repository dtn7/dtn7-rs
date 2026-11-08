//! PRoPHET v2 routing, derived from Grasic et al., 2011 and RFC 6693.
//!
//! This agent implements PRoPHETv2 using dtn7-rs routing metadata bundles
//! (see `routing::metadata`) instead of the RFC's TCP-based protocol.
//! It also omits optional features such as EWMA smoothing.
//!
//! Bundle forwarding uses GRTR:
//! a bundle for destination D is forwarded to peer B if `P(B,D) > P(A,D)`,
//! with a direct-delivery override when B is the destination itself.
//! GRTR is only defined for singleton destinations, so group bundles are spread epidemically.
//!
//! Metric updates follow RFC 6693 section 2.1.2:
//! - a first encounter sets `P(A,B)` to `p_encounter_first`;
//! - later encounters use the interval-scaled Equation 1;
//! - Equation 2 ages predictabilities by `gamma^K`;
//! - Equation 3 uses `max(P_old, P(A,B) * P(B,C) * beta)`.
//!
//! The contact-tracking tick only detects candidate contacts and
//! schedules routing-information exchanges; it never applies Eq. 1 or Eq. 3.
//! A valid received RIB is an information-exchange update event.
//! At that point Eq. 1 is applied to all confirmed active contacts
//! using the simultaneous-contact rules SC2/SC3 in RFC 6693 section 3.2.2,
//! followed by Eq. 3 for the sender.

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

const TAG: &str = "prophet_v2";
const PROPHET_RIB_VERSION: u8 = 2;

/// Configuration for the PRoPHET v2 routing agent.
#[derive(Debug, Clone)]
pub struct ProphetV2Config {
    /// Maximum encounter scaling factor of Equation 1.
    pub p_encounter_max: f64,
    /// Predictability assigned on a first encounter.
    pub p_encounter_first: f64,
    /// Values below this threshold are treated as unknown and removed.
    pub p_first_threshold: f64,
    /// Transitivity scaling constant of Equation 3.
    pub beta: f64,
    /// Aging constant of Equation 2.
    pub gamma: f64,
    /// Difference between one and the maximum non-self predictability.
    pub delta: f64,
    /// Typical interval between independent encounters.
    pub i_typ: Duration,
    /// Duration of one aging time unit K.
    pub time_unit: Duration,
    /// Interval between RIB exchanges during a persistent contact.
    pub exchange_interval: Duration,
    /// Interval used to track contacts and schedule exchanges.
    pub tick_interval: Duration,
    /// Lifetime of administrative bundles carrying RIBs.
    pub record_lifetime: Duration,
}

impl Default for ProphetV2Config {
    /// Defaults are based on RFC 6693, Figure 3;
    /// values not specified by the RFC were chosen here.
    fn default() -> Self {
        ProphetV2Config {
            p_encounter_max: 0.7,
            p_encounter_first: 0.5,
            p_first_threshold: 0.1,
            beta: 0.9,
            gamma: 0.999,
            delta: 0.01,
            i_typ: Duration::from_secs(5 * 60),
            time_unit: Duration::from_secs(30),
            exchange_interval: Duration::from_secs(60),
            tick_interval: Duration::from_secs(5),
            record_lifetime: Duration::from_secs(60),
        }
    }
}

impl ProphetV2Config {
    /// Builds a config from the `prophet_v2` section of the routing settings,
    /// panicking on invalid values so misconfiguration fails at startup.
    pub fn from_settings(settings: Option<&HashMap<String, String>>) -> ProphetV2Config {
        let mut cfg = ProphetV2Config::default();
        let empty = HashMap::new();
        let settings = settings.unwrap_or(&empty);

        cfg.p_encounter_max =
            parse_probability(TAG, settings, "p_encounter_max", cfg.p_encounter_max);
        cfg.p_encounter_first =
            parse_probability(TAG, settings, "p_encounter_first", cfg.p_encounter_first);
        cfg.p_first_threshold =
            parse_probability(TAG, settings, "p_first_threshold", cfg.p_first_threshold);
        cfg.beta = parse_probability(TAG, settings, "beta", cfg.beta);
        cfg.gamma = parse_probability(TAG, settings, "gamma", cfg.gamma);
        cfg.delta = parse_probability(TAG, settings, "delta", cfg.delta);
        cfg.i_typ = parse_duration(TAG, settings, "i_typ", cfg.i_typ);
        cfg.time_unit = parse_duration(TAG, settings, "time_unit", cfg.time_unit);
        cfg.exchange_interval =
            parse_duration(TAG, settings, "exchange_interval", cfg.exchange_interval);
        cfg.tick_interval = parse_duration(TAG, settings, "tick_interval", cfg.tick_interval);
        cfg.record_lifetime = parse_duration(TAG, settings, "record_lifetime", cfg.record_lifetime);

        if cfg.delta <= 0.0 || cfg.delta >= 1.0 {
            panic!(
                "{}: delta must be greater than zero and smaller than one",
                TAG
            );
        }
        if cfg.p_first_threshold >= cfg.p_encounter_first {
            panic!(
                "{}: p_first_threshold must be smaller than p_encounter_first",
                TAG
            );
        }
        if cfg.p_encounter_first > 1.0 - cfg.delta {
            panic!("{}: p_encounter_first must not exceed 1 - delta", TAG);
        }
        if cfg.i_typ.is_zero()
            || cfg.time_unit.is_zero()
            || cfg.exchange_interval.is_zero()
            || cfg.tick_interval.is_zero()
            || cfg.record_lifetime.is_zero()
        {
            panic!("{}: duration settings must be greater than zero", TAG);
        }
        cfg
    }
}

/// A configured or discovered peer that may currently be reachable.
#[derive(Debug)]
struct Contact {
    /// The peer's node endpoint.
    eid: EndpointID,
    /// Confirmed by a successful outgoing RIB or a valid incoming RIB.
    confirmed: bool,
    /// The previous encounter interval to use at the next metric update.
    first_interval: Option<Duration>,
    /// Whether `first_interval` still needs to be applied according to SC2.
    needs_first_update: bool,
    /// Last time a RIB transfer was attempted.
    last_attempt: Option<Instant>,
    /// A RIB bundle is currently being forwarded to this peer.
    in_flight: bool,
}

impl Contact {
    fn candidate(eid: EndpointID) -> Contact {
        Contact {
            eid,
            confirmed: false,
            first_interval: None,
            needs_first_update: false,
            last_attempt: None,
            in_flight: false,
        }
    }
}

/// This node's delivery predictabilities, aged lazily on access.
struct DeliveryPredictabilities {
    preds: HashMap<String, f64>,
    last_aged: Instant,
    cfg: ProphetV2Config,
}

impl DeliveryPredictabilities {
    fn new(cfg: ProphetV2Config, now: Instant) -> DeliveryPredictabilities {
        DeliveryPredictabilities {
            preds: HashMap::new(),
            last_aged: now,
            cfg,
        }
    }

    /// Applies Equation 1 to a directly contacted peer.
    fn encounter(&mut self, peer: &str, interval: Option<Duration>, now: Instant) {
        self.age(now);
        let p = self.preds.entry(peer.to_string()).or_insert(0.0);
        if *p < self.cfg.p_first_threshold {
            *p = self.cfg.p_encounter_first;
            return;
        }
        let p_encounter = interval.map_or(self.cfg.p_encounter_max, |interval| {
            self.cfg.p_encounter_max
                * (interval.as_secs_f64() / self.cfg.i_typ.as_secs_f64()).min(1.0)
        });
        *p += (1.0 - self.cfg.delta - *p) * p_encounter;
    }

    /// Applies the aging Equation 2 to all known predictabilities.
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
        self.preds.retain(|_, p| *p >= self.cfg.p_first_threshold);
    }

    /// Applies the max-form transitivity rule of Equation 3.
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
            *p = p.max(p_ab * p_bc * self.cfg.beta);
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
pub struct ProphetV2RoutingAgent {
    tx: mpsc::Sender<RoutingCmd>,
}

impl Default for ProphetV2RoutingAgent {
    fn default() -> Self {
        ProphetV2RoutingAgent::new()
    }
}

impl ProphetV2RoutingAgent {
    pub fn new() -> ProphetV2RoutingAgent {
        let settings = CONFIG.lock().routing_settings.clone();
        let cfg = ProphetV2Config::from_settings(settings.get(TAG));

        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            handle_routing_cmd(cfg, rx).await;
        });

        ProphetV2RoutingAgent { tx }
    }
}

impl std::fmt::Display for ProphetV2RoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "ProphetV2RoutingAgent")
    }
}

#[async_trait]
impl RoutingAgent for ProphetV2RoutingAgent {
    fn channel(&self) -> Sender<RoutingCmd> {
        self.tx.clone()
    }
}

struct ProphetV2RoutingAgentCore {
    /// The configuration for this agent.
    cfg: ProphetV2Config,
    /// The local node name (from the host EID).
    local_node: String,
    /// The EID of this node.
    host_eid: EndpointID,
    /// Delivery predictabilities for this node P(A,x).
    dps: DeliveryPredictabilities,
    /// Configured or discovered peers that may currently be reachable.
    contacts: HashMap<String, Contact>,
    /// Start time of the most recently confirmed communication session per peer,
    /// as specified for the recent-encounter list in RFC 6693 section 3.1.
    /// SC3 updates during that session do not replace this timestamp.
    recent_encounters: HashMap<String, Instant>,
    /// Time of the previous simultaneous-contact metric update (SC3).
    last_metric_update: Instant,
    /// Last received RIB per peer.
    /// Values are used as advertised until refreshed.
    peer_ribs: HashMap<String, (HashMap<String, f64>, Instant)>,
    /// RIB bundle ID to target peer.
    metadata_bundles: HashMap<String, String>,
    /// Application bundles already sent to each peer.
    history: HashMap<String, BundleHistory>,
}

impl ProphetV2RoutingAgentCore {
    fn new(cfg: ProphetV2Config) -> ProphetV2RoutingAgentCore {
        let host_eid = CONFIG.lock().host_eid.clone();
        let now = Instant::now();
        ProphetV2RoutingAgentCore::new_with_host(cfg, host_eid, now)
    }

    fn new_with_host(
        cfg: ProphetV2Config,
        host_eid: EndpointID,
        now: Instant,
    ) -> ProphetV2RoutingAgentCore {
        ProphetV2RoutingAgentCore {
            local_node: host_eid.node().expect("invalid local node EID"),
            host_eid,
            dps: DeliveryPredictabilities::new(cfg.clone(), now),
            cfg,
            contacts: HashMap::new(),
            recent_encounters: HashMap::new(),
            last_metric_update: now,
            peer_ribs: HashMap::new(),
            metadata_bundles: HashMap::new(),
            history: HashMap::new(),
        }
    }

    /// Adds a peer as an exchange candidate without changing its metric.
    ///
    /// Returns true if this is a new candidate.
    /// The caller decides when to send the first RIB
    /// so an inbound RIB can be processed before its reply is constructed.
    fn candidate_contact(&mut self, peer_name: &str, eid: &EndpointID) -> bool {
        if peer_name == self.local_node {
            return false;
        }
        if let Some(contact) = self.contacts.get_mut(peer_name) {
            contact.eid = eid.clone();
            return false;
        }
        debug!("{}: found candidate peer {}", TAG, peer_name);
        self.contacts
            .insert(peer_name.to_string(), Contact::candidate(eid.clone()));
        true
    }

    /// Marks a proven communication opportunity as an active contact.
    fn confirm_contact(&mut self, peer_name: &str, now: Instant) {
        let Some(contact) = self.contacts.get_mut(peer_name) else {
            return;
        };
        if contact.confirmed {
            return;
        }
        debug!("{}: confirmed contact with {}", TAG, peer_name);
        contact.confirmed = true;
        contact.first_interval = self
            .recent_encounters
            .insert(peer_name.to_string(), now)
            .map(|previous| now.saturating_duration_since(previous));
        contact.needs_first_update = true;
    }

    /// Removes a peer from the contacts map.
    fn contact_lost(&mut self, peer_name: &str) {
        if self.contacts.remove(peer_name).is_some() {
            debug!("{}: lost contact with peer {}", TAG, peer_name);
        }
        // Discard the cached RIB with the contact.
        // Otherwise, a successful outbound transfer during a later contact could mark the peer confirmed
        // and make stale values eligible for GRTR before a fresh RIB arrives.
        self.peer_ribs.remove(peer_name);
    }

    /// Marks a failed RIB exchange as loss of confirmed reachability without
    /// changing the delivery predictability learned from earlier encounters.
    fn rib_send_failed(&mut self, peer_name: &str) {
        if let Some(contact) = self.contacts.get_mut(peer_name) {
            contact.in_flight = false;
            contact.confirmed = false;
            contact.needs_first_update = false;
        }
        self.peer_ribs.remove(peer_name);
    }

    /// Sends an aged predictability-map snapshot without applying an encounter update.
    /// This happens when a candidate first appears,
    /// as an immediate reply to a newly established inbound exchange,
    /// and periodically while a contact persists.
    fn send_rib(&mut self, peer_name: &str, now: Instant) {
        let Some(peer_eid) = self.contacts.get(peer_name).and_then(|contact| {
            if contact.in_flight {
                None
            } else {
                Some(contact.eid.clone())
            }
        }) else {
            return;
        };

        let mut preds = self.dps.snapshot(now);
        // RFC 6693 defines P(A,A) as one.
        // It is not stored locally because it is immutable,
        // but advertising it makes the RIB representation complete.
        preds.insert(self.local_node.clone(), 1.0);
        let rib = ProphetRib {
            version: PROPHET_RIB_VERSION,
            node: self.local_node.clone(),
            preds,
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
                if let Some(contact) = self.contacts.get_mut(peer_name) {
                    contact.last_attempt = Some(now);
                    contact.in_flight = true;
                }
                // The record reaches the store before this returns,
                // so `prune_gone_bundles` can never see it as gone while it is still tracked here.
                if let Err(err) = metadata::send_routing_record_bundle(bndl) {
                    warn!(
                        "{}: could not send predictability map to {}: {}",
                        TAG, peer_name, err
                    );
                    // Nothing was sent, so no transfer result will ever arrive for it.
                    self.metadata_bundles.remove(&bid);
                    if let Some(contact) = self.contacts.get_mut(peer_name) {
                        contact.in_flight = false;
                    }
                }
            }
            Err(err) => warn!("{}: could not encode predictability map: {}", TAG, err),
        }
    }

    /// Applies RFC 6693 SC2/SC3 to every confirmed active contact.
    fn update_active_contacts(&mut self, now: Instant) {
        let shared_interval = now.saturating_duration_since(self.last_metric_update);
        let updates: Vec<(String, Option<Duration>, bool)> = self
            .contacts
            .iter()
            .filter(|(_, contact)| contact.confirmed)
            .map(|(name, contact)| {
                (
                    name.clone(),
                    if contact.needs_first_update {
                        contact.first_interval
                    } else {
                        Some(shared_interval)
                    },
                    contact.needs_first_update,
                )
            })
            .collect();

        for (name, interval, was_first) in updates {
            self.dps.encounter(&name, interval, now);
            if was_first && let Some(contact) = self.contacts.get_mut(&name) {
                contact.needs_first_update = false;
            }
        }
        self.last_metric_update = now;
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
            self.candidate_contact(name, eid);
        }

        let due: Vec<String> = self
            .contacts
            .iter()
            .filter(|(_, contact)| {
                !contact.in_flight
                    && contact.last_attempt.is_none_or(|last| {
                        now.saturating_duration_since(last) >= self.cfg.exchange_interval
                    })
            })
            .map(|(name, _)| name.clone())
            .collect();
        for name in due {
            self.send_rib(&name, now);
        }

        let active: HashSet<&String> = self
            .contacts
            .iter()
            .filter(|(_, contact)| contact.confirmed)
            .map(|(name, _)| name)
            .collect();
        self.recent_encounters.retain(|name, encountered| {
            active.contains(name) || now.saturating_duration_since(*encountered) <= self.cfg.i_typ
        });
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
            if let Some(peer_name) = self.metadata_bundles.remove(&bid)
                && let Some(contact) = self.contacts.get_mut(&peer_name)
            {
                contact.in_flight = false;
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

        let was_confirmed = self
            .contacts
            .get(&name)
            .is_some_and(|contact| contact.confirmed);
        self.candidate_contact(&name, src);
        self.confirm_contact(&name, now);
        // An accepted RIB is an Information Exchange update event.
        // Apply SC2/SC3 here; the contact tick only schedules exchanges and
        // must not update metrics without receiving fresh routing information.
        self.update_active_contacts(now);
        self.dps
            .apply_transitivity(&name, &rib.preds, &self.local_node.clone(), now);
        self.peer_ribs.insert(name.clone(), (rib.preds, now));

        // An inbound record can prove that a previously failed static contact is reachable.
        // Reply immediately with the freshly updated map instead of waiting for its old retry deadline.
        if !was_confirmed
            && self
                .contacts
                .get(&name)
                .is_some_and(|contact| !contact.in_flight)
        {
            self.send_rib(&name, now);
        }
    }

    /// Returns the delivery predictability of a peer towards a destination, or zero if unknown.
    fn peer_pred(&self, peer_name: &str, dest: &str) -> f64 {
        self.peer_ribs
            .get(peer_name)
            .and_then(|(preds, _)| preds.get(dest))
            .copied()
            .unwrap_or(0.0)
    }

    /// Handles a routing notification.
    fn handle_notification(&mut self, notification: RoutingNotification) {
        let now = Instant::now();
        match notification {
            RoutingNotification::EncounteredPeer(eid) => {
                if let Some(name) = eid.node()
                    && self.candidate_contact(&name, &eid)
                {
                    self.send_rib(&name, now);
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
                    self.rib_send_failed(&peer_name);
                    debug!(
                        "{}: sending predictability map {} to {} failed",
                        TAG, bid, node_name
                    );
                } else if let Some(history) = self.history.get_mut(&bid) {
                    // Allow a retry towards this peer.
                    history.nodes.remove(&node_name);
                }
            }
            RoutingNotification::SendingSucceeded(bid, _) => {
                if let Some(peer_name) = self.metadata_bundles.remove(&bid) {
                    if let Some(contact) = self.contacts.get_mut(&peer_name) {
                        contact.in_flight = false;
                    }
                    self.confirm_contact(&peer_name, now);
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

            // A configured static peer is only a candidate until a RIB transfer succeeds in either direction.
            // Do not route through stale RIB data after a later exchange failure.
            if !self
                .contacts
                .get(&peer_name)
                .is_some_and(|contact| contact.confirmed)
            {
                continue;
            }

            let p_peer = self.peer_pred(&peer_name, &dest);
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

        let mut clas = Vec::with_capacity(selected.len());
        for (peer_name, cla) in selected {
            history.nodes.insert(peer_name);
            clas.push(cla);
        }
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
                        "confirmed": contact.confirmed,
                        "in_flight": contact.in_flight,
                    }),
                )
            })
            .collect();
        serde_json::json!({
            "config": {
                "p_encounter_max": self.cfg.p_encounter_max,
                "p_encounter_first": self.cfg.p_encounter_first,
                "p_first_threshold": self.cfg.p_first_threshold,
                "beta": self.cfg.beta,
                "gamma": self.cfg.gamma,
                "delta": self.cfg.delta,
                "i_typ_secs": self.cfg.i_typ.as_secs_f64(),
                "time_unit_secs": self.cfg.time_unit.as_secs_f64(),
                "exchange_interval_secs": self.cfg.exchange_interval.as_secs_f64(),
            },
            "local": self.dps.snapshot(now),
            "peer_ribs": peer_ribs,
            "contacts": contacts,
        })
        .to_string()
    }
}

/// The main loop of the PRoPHET routing agent, running in its own task.
async fn handle_routing_cmd(cfg: ProphetV2Config, mut rx: mpsc::Receiver<RoutingCmd>) {
    if cfg.exchange_interval >= cfg.i_typ {
        warn!(
            "{}: exchange_interval ({:?}) is not smaller than i_typ ({:?}); each periodic information exchange can receive the full p_encounter_max credit",
            TAG, cfg.exchange_interval, cfg.i_typ
        );
    }
    info!("{}: starting with {:?}", TAG, cfg);

    let mut core = ProphetV2RoutingAgentCore::new(cfg);
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
                    RoutingCmd::Notify(notification) => {
                        core.handle_notification(notification);
                    }
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
    fn first_encounter_sets_p_encounter_first() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg, now);
        dps.encounter("b", None, now);
        assert_close(dps.get("b", now), 0.5);
    }

    #[test]
    fn repeated_encounter_scales_with_interval_and_caps_at_i_typ() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg.clone(), now);
        dps.encounter("b", None, now);

        dps.encounter("b", Some(cfg.i_typ / 2), now);
        let expected = 0.5 + (1.0 - 0.01 - 0.5) * 0.7 * 0.5;
        assert_close(dps.get("b", now), expected);

        dps.encounter("c", None, now);
        dps.encounter("c", Some(cfg.i_typ * 2), now);
        assert_close(dps.get("c", now), 0.5 + (1.0 - 0.01 - 0.5) * 0.7);
    }

    #[test]
    fn zero_interval_is_a_noop_after_first_encounter() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg, now);
        dps.encounter("b", None, now);
        dps.encounter("b", Some(Duration::ZERO), now);
        assert_close(dps.get("b", now), 0.5);
    }

    #[test]
    fn predictabilities_remain_below_one_minus_delta() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg.clone(), now);
        dps.encounter("b", None, now);
        for _ in 0..100 {
            dps.encounter("b", Some(cfg.i_typ), now);
        }
        let probability = dps.get("b", now);
        assert!(probability < 1.0 - cfg.delta + 1e-12);
        assert!(probability > 0.98);
    }

    #[test]
    fn aging_prunes_below_first_threshold() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg.clone(), now);
        dps.encounter("b", None, now);
        let later = now + cfg.time_unit * 1610;
        assert_close(dps.get("b", later), 0.0);
        dps.encounter("b", Some(cfg.i_typ), later);
        assert_close(dps.get("b", later), cfg.p_encounter_first);
    }

    #[test]
    fn transitivity_uses_max_and_does_not_accumulate() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let mut dps = DeliveryPredictabilities::new(cfg, now);
        dps.encounter("b", None, now);
        let received: HashMap<String, f64> = [("c".to_string(), 0.8)].into();
        dps.apply_transitivity("b", &received, "a", now);
        let expected = 0.5 * 0.8 * 0.9;
        assert_close(dps.get("c", now), expected);
        for _ in 0..100 {
            dps.apply_transitivity("b", &received, "a", now);
        }
        assert_close(dps.get("c", now), expected);
    }

    #[test]
    fn config_defaults_and_overrides() {
        let cfg = ProphetV2Config::from_settings(None);
        assert_close(cfg.p_encounter_max, 0.7);
        assert_close(cfg.p_encounter_first, 0.5);
        assert_close(cfg.p_first_threshold, 0.1);
        assert_close(cfg.beta, 0.9);
        assert_close(cfg.gamma, 0.999);
        assert_close(cfg.delta, 0.01);
        assert_eq!(cfg.i_typ, Duration::from_secs(300));

        let settings: HashMap<String, String> = [("i_typ".to_string(), "10m".to_string())].into();
        let cfg = ProphetV2Config::from_settings(Some(&settings));
        assert_eq!(cfg.i_typ, Duration::from_secs(600));
    }

    #[test]
    #[should_panic(expected = "p_first_threshold must be smaller")]
    fn config_rejects_invalid_threshold_relationship() {
        let settings: HashMap<String, String> =
            [("p_first_threshold".to_string(), "0.6".to_string())].into();
        ProphetV2Config::from_settings(Some(&settings));
    }

    #[test]
    #[should_panic(expected = "delta must be greater than zero")]
    fn config_rejects_zero_delta() {
        let settings: HashMap<String, String> = [("delta".to_string(), "0".to_string())].into();
        ProphetV2Config::from_settings(Some(&settings));
    }

    #[test]
    fn simultaneous_contacts_share_the_update_interval() {
        let cfg = ProphetV2Config {
            time_unit: Duration::from_secs(3600),
            ..Default::default()
        };
        let t0 = Instant::now();
        let host: EndpointID = "dtn://a/".try_into().unwrap();
        let mut core = ProphetV2RoutingAgentCore::new_with_host(cfg.clone(), host, t0);

        core.dps.encounter("b", None, t0);
        core.dps.encounter("c", None, t0);
        core.contacts.insert(
            "b".to_string(),
            Contact {
                eid: "dtn://b/".try_into().unwrap(),
                confirmed: true,
                first_interval: None,
                needs_first_update: false,
                last_attempt: None,
                in_flight: false,
            },
        );
        core.contacts.insert(
            "c".to_string(),
            Contact {
                eid: "dtn://c/".try_into().unwrap(),
                confirmed: true,
                first_interval: None,
                needs_first_update: false,
                last_attempt: None,
                in_flight: false,
            },
        );

        let later = t0 + cfg.i_typ / 2;
        core.update_active_contacts(later);
        let expected = 0.5 + (1.0 - cfg.delta - 0.5) * cfg.p_encounter_max * 0.5;
        assert_close(core.dps.get("b", later), expected);
        assert_close(core.dps.get("c", later), expected);
    }

    #[test]
    fn new_contact_uses_its_own_previous_encounter_interval() {
        let cfg = ProphetV2Config {
            time_unit: Duration::from_secs(3600),
            ..Default::default()
        };
        let t0 = Instant::now();
        let host: EndpointID = "dtn://a/".try_into().unwrap();
        let mut core = ProphetV2RoutingAgentCore::new_with_host(cfg.clone(), host, t0);
        core.dps.encounter("b", None, t0);
        core.contacts.insert(
            "b".to_string(),
            Contact {
                eid: "dtn://b/".try_into().unwrap(),
                confirmed: true,
                first_interval: Some(cfg.i_typ),
                needs_first_update: true,
                last_attempt: None,
                in_flight: false,
            },
        );

        // The global SC3 interval is short, but the restarted contact's SC2
        // interval is I_typ and must therefore use p_encounter_max.
        let later = t0 + Duration::from_secs(1);
        core.update_active_contacts(later);
        let expected = 0.5 + (1.0 - cfg.delta - 0.5) * cfg.p_encounter_max;
        assert_close(core.dps.get("b", later), expected);
    }

    #[test]
    fn mismatched_rib_version_is_ignored_before_contact_confirmation() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let host: EndpointID = "dtn://a/".try_into().unwrap();
        let mut core = ProphetV2RoutingAgentCore::new_with_host(cfg, host, now);
        let src: EndpointID = "dtn://b/".try_into().unwrap();
        let rib = ProphetRib {
            version: 1,
            node: "b".to_string(),
            preds: HashMap::new(),
        };
        core.handle_rib(&src, &serde_cbor::to_vec(&rib).unwrap(), now);
        assert!(core.contacts.is_empty());
        assert!(core.peer_ribs.is_empty());
    }

    #[test]
    fn self_sourced_rib_is_ignored() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let host: EndpointID = "dtn://a/".try_into().unwrap();
        let mut core = ProphetV2RoutingAgentCore::new_with_host(cfg, host.clone(), now);
        let rib = ProphetRib {
            version: PROPHET_RIB_VERSION,
            node: "a".to_string(),
            preds: [("b".to_string(), 0.5)].into(),
        };

        core.handle_rib(&host, &serde_cbor::to_vec(&rib).unwrap(), now);

        assert!(core.contacts.is_empty());
        assert!(core.peer_ribs.is_empty());
    }

    #[test]
    fn candidate_creation_does_not_preempt_a_fresh_rib_reply() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let host: EndpointID = "dtn://a/".try_into().unwrap();
        let mut core = ProphetV2RoutingAgentCore::new_with_host(cfg, host, now);
        let peer: EndpointID = "dtn://b/".try_into().unwrap();

        assert!(core.candidate_contact("b", &peer));
        assert!(!core.contacts["b"].in_flight);
        assert!(core.metadata_bundles.is_empty());
    }

    #[test]
    fn contact_loss_discards_the_peers_last_rib() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let host: EndpointID = "dtn://a/".try_into().unwrap();
        let mut core = ProphetV2RoutingAgentCore::new_with_host(cfg, host, now);
        let peer: EndpointID = "dtn://b/".try_into().unwrap();
        core.candidate_contact("b", &peer);
        core.peer_ribs
            .insert("b".to_string(), ([("c".to_string(), 0.7)].into(), now));

        core.contact_lost("b");

        assert!(!core.contacts.contains_key("b"));
        assert!(!core.peer_ribs.contains_key("b"));
    }

    #[test]
    fn failed_static_exchange_does_not_increase_predictability() {
        let cfg = ProphetV2Config::default();
        let now = Instant::now();
        let host: EndpointID = "dtn://a/".try_into().unwrap();
        let mut core = ProphetV2RoutingAgentCore::new_with_host(cfg, host, now);
        core.dps.encounter("b", None, now);
        core.contacts.insert(
            "b".to_string(),
            Contact {
                eid: "dtn://b/".try_into().unwrap(),
                confirmed: true,
                first_interval: None,
                needs_first_update: false,
                last_attempt: Some(now),
                in_flight: true,
            },
        );
        core.peer_ribs
            .insert("b".to_string(), ([("c".to_string(), 0.7)].into(), now));

        let before = core.dps.get("b", now);
        core.rib_send_failed("b");
        assert_close(core.dps.get("b", now), before);
        assert!(!core.contacts["b"].confirmed);
        assert!(!core.contacts["b"].in_flight);
        assert!(!core.peer_ribs.contains_key("b"));
    }
}
