use std::collections::HashMap;

use super::RoutingAgent;
use crate::cla::ClaSenderTask;
use crate::core::bundlepack::BundlePack;
use crate::routing::RoutingCmd;
use crate::{RoutingNotification, CONFIG, PEERS};
use async_trait::async_trait;
use log::{debug, info, warn};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

/// Simple implementation of basic spray and wait routing.
/// For each bundle only l copies are spread after which we enter a wait phase.
/// Then only direct delivery is possible.
#[derive(Debug)]
pub struct SprayAndWaitRoutingAgent {
    tx: mpsc::Sender<super::RoutingCmd>,
}

#[derive(Debug)]
pub struct SaWBundleData {
    /// the number of copies we have left to spread
    remaining_copies: usize,
    /// the node names of the peers that have already received the bundle
    nodes: Vec<String>,
}

/// The default number of copies that are can be sent to peers.
const MAX_COPIES: usize = 7;

struct SprayAndWaitRoutingAgentCore {
    /// the number of copies we have left to spread
    l: usize,
    /// for each bundle ID we store the number of copies we have left and the already nodes that already received a copy
    history: HashMap<String, SaWBundleData>,
    /// our local node ID to identify our own bundles
    local_node: String,
}

impl SprayAndWaitRoutingAgentCore {
    pub fn new(starting_copies: usize) -> SprayAndWaitRoutingAgentCore {
        SprayAndWaitRoutingAgentCore {
            l: starting_copies,
            history: HashMap::new(),
            local_node: crate::CONFIG.lock().host_eid.node_id().unwrap(),
        }
    }
    /// Prepare new bundles for spreading.
    pub fn handle_new_bundle(&mut self, bundle_id: String) {
        if self.history.contains_key(&bundle_id) {
            // keep the remaining copies and the nodes that already received the bundle
            debug!("Bundle {} already known", bundle_id);
            return;
        }
        if bundle_id.starts_with(&self.local_node) {
            // this is our own bundle, thus, we have l copies to spread
            let meta = SaWBundleData {
                remaining_copies: self.l,
                nodes: Vec::new(),
            };
            debug!("Adding new bundle {} from this host", bundle_id);
            self.history.insert(bundle_id, meta);
        } else {
            // this is a bundle from another host, thus, we have only one copy
            let meta = SaWBundleData {
                remaining_copies: 1,
                nodes: Vec::new(),
            };
            debug!("Adding bundle {} from foreign host", bundle_id);
            self.history.insert(bundle_id, meta);
        }
    }
}

impl Default for SprayAndWaitRoutingAgent {
    fn default() -> Self {
        SprayAndWaitRoutingAgent::new()
    }
}

impl SprayAndWaitRoutingAgent {
    pub fn new() -> SprayAndWaitRoutingAgent {
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            handle_routing_cmd(rx).await;
        });

        SprayAndWaitRoutingAgent { tx }
    }
}
impl std::fmt::Display for SprayAndWaitRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "SprayAndWaitRoutingAgent")
    }
}

#[async_trait]
impl RoutingAgent for SprayAndWaitRoutingAgent {
    fn channel(&self) -> Sender<RoutingCmd> {
        self.tx.clone()
    }
}

fn handle_notification(core: &mut SprayAndWaitRoutingAgentCore, notification: RoutingNotification) {
    match notification {
        RoutingNotification::SendingFailed(bid, next_hop_node_name) => {
            // If a transmission fails we have to remove the node from the list of already received nodes
            // and increase the number of remaining copies again.
            // Direct deliveries do not consume a copy and are not listed, so nothing is
            // returned for them.
            if let Some(meta) = core.history.get_mut(&bid) {
                let old_size = meta.nodes.len();
                meta.nodes
                    .retain(|node_name| node_name != &next_hop_node_name);
                let removed = old_size - meta.nodes.len();
                if removed > 1 {
                    warn!(
                        "Removed {} entries of {} from bid {}, duplicate entries, should be 1!",
                        removed, next_hop_node_name, bid
                    );
                }
                meta.remaining_copies += removed;
            }
        }
        RoutingNotification::IncomingBundle(bndl) => {
            core.handle_new_bundle(bndl.id());
        }
        RoutingNotification::IncomingBundleWithoutPreviousNode(bid, _node_name) => {
            core.handle_new_bundle(bid);
        }
        _ => {}
    }
}
async fn handle_sender_for_bundle(
    core: &mut SprayAndWaitRoutingAgentCore,
    bp: BundlePack,
    reply: tokio::sync::oneshot::Sender<(Vec<ClaSenderTask>, bool)>,
) {
    let mut clas = Vec::new();
    let mut delete_afterwards = false;

    if let Some(meta) = core.history.get_mut(bp.id()) {
        for p in (*PEERS.lock()).values() {
            let Some(peer_node_id) = p.eid.node_id() else {
                continue;
            };
            // node names match the next hop names of SendingFailed notifications
            let peer_node_name = p.node_name();
            if peer_node_id == core.local_node || meta.nodes.contains(&peer_node_name) {
                // skip if the peer is ourself or if we already sent the bundle to this peer
                continue;
            }
            if meta.remaining_copies < 2 {
                // we are done with this bundle, only direct delivery remains
                if bp.destination.node().as_deref() == Some(p.node_name().as_str()) {
                    // direct delivery possible
                    debug!(
                        "Attempting direct delivery of bundle {} to {}",
                        bp.id(),
                        p.node_name()
                    );
                    if let Some(cla) = p.first_cla() {
                        delete_afterwards = true;
                        clas.clear();
                        clas.push(cla);
                    }
                } else {
                    debug!(
                        "Not relaying bundle {} any more because there are no copies left",
                        bp.id()
                    );
                }
                continue;
            }
            if let Some(cla) = p.first_cla() {
                clas.push(cla);
                meta.remaining_copies -= 1;
                meta.nodes.push(peer_node_name);
            }
            debug!(
                "Relaying bundle {} to {}, {} copies remaining",
                bp.id(),
                peer_node_id,
                meta.remaining_copies
            );
        }
    } else {
        warn!("Bundle {} not found", bp.id());
    }
    tokio::spawn(async move {
        // the requester may already be gone (e.g., timed out), which is not an error here
        let _ = reply.send((clas, delete_afterwards));
    });
}
async fn handle_routing_cmd(mut rx: mpsc::Receiver<RoutingCmd>) {
    let settings = CONFIG.lock().routing_settings.clone();

    let max_copies = match settings
        .get("sprayandwait")
        .and_then(|settings| settings.get("num_copies"))
    {
        Some(num_copies) => num_copies.parse::<usize>().unwrap_or_else(|_| {
            warn!(
                "invalid sprayandwait.num_copies '{}', using default of {}",
                num_copies, MAX_COPIES
            );
            MAX_COPIES
        }),
        None => MAX_COPIES,
    };
    info!("configured to allow {} copies", max_copies);

    let mut core: SprayAndWaitRoutingAgentCore = SprayAndWaitRoutingAgentCore::new(max_copies);
    while let Some(cmd) = rx.recv().await {
        match cmd {
            super::RoutingCmd::SenderForBundle(bp, reply) => {
                handle_sender_for_bundle(&mut core, bp, reply).await;
            }
            super::RoutingCmd::Shutdown => {
                break;
            }
            super::RoutingCmd::Command(_cmd) => {}
            super::RoutingCmd::GetData(_, tx) => {
                // the requester may already be gone (e.g., timed out), which is not an error here
                let _ = tx.send(format!("{:?}", core.history));
            }
            super::RoutingCmd::Notify(notification) => {
                handle_notification(&mut core, notification);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core_with(
        bid: &str,
        remaining_copies: usize,
        nodes: &[&str],
    ) -> SprayAndWaitRoutingAgentCore {
        let mut core = SprayAndWaitRoutingAgentCore::new(MAX_COPIES);
        core.history.insert(
            bid.to_string(),
            SaWBundleData {
                remaining_copies,
                nodes: nodes.iter().map(|n| n.to_string()).collect(),
            },
        );
        core
    }

    #[test]
    fn sending_failed_returns_copy_of_failed_node_only() {
        let mut core = core_with("bid", 0, &["node2", "node20", "node3"]);
        handle_notification(
            &mut core,
            RoutingNotification::SendingFailed("bid".into(), "node2".into()),
        );
        let meta = &core.history["bid"];
        assert_eq!(meta.nodes, vec!["node20", "node3"]);
        assert_eq!(meta.remaining_copies, 1);
    }

    #[test]
    fn sending_failed_for_unlisted_node_keeps_copies() {
        // e.g., a failed direct delivery, which did not consume a copy
        let mut core = core_with("bid", 0, &["node3"]);
        handle_notification(
            &mut core,
            RoutingNotification::SendingFailed("bid".into(), "node2".into()),
        );
        let meta = &core.history["bid"];
        assert_eq!(meta.nodes, vec!["node3"]);
        assert_eq!(meta.remaining_copies, 0);
    }

    #[test]
    fn known_bundles_are_not_reset() {
        let mut core = core_with("bid", 2, &["node3"]);
        core.handle_new_bundle("bid".into());
        let meta = &core.history["bid"];
        assert_eq!(meta.nodes, vec!["node3"]);
        assert_eq!(meta.remaining_copies, 2);
    }
}
