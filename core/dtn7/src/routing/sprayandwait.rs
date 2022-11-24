use std::collections::HashMap;

use super::RoutingAgent;
use crate::cla::ClaSenderTask;
use crate::core::bundlepack::BundlePack;
use crate::routing::RoutingCmd;
use crate::{RoutingNotifcation, CONFIG, PEERS};
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

pub struct SaWBundleData {
    /// the number of copies we have left to spread
    remaining_copies: usize,
    /// the list of nodes that have already received the bundle
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
        if bundle_id.starts_with(&self.local_node) {
            // this is our own bundle, thus, we have l copies to spread
            let meta = SaWBundleData {
                remaining_copies: self.l,
                nodes: Vec::new(),
            };
            debug!("Adding new bundle {} from this host", &bundle_id);
            self.history.insert(bundle_id, meta);
        } else {
            // this is a bundle from another host, thus, we have only one copy
            let meta = SaWBundleData {
                remaining_copies: 1,
                nodes: Vec::new(),
            };
            debug!("Adding bundle {} from foreign host", &bundle_id);
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

fn handle_notification(core: &mut SprayAndWaitRoutingAgentCore, notification: RoutingNotifcation) {
    match notification {
        RoutingNotifcation::SendingFailed(bid, next_hop_node_name) => {
            // If a transmission fails we have to remove the node from the list of already received nodes
            // and increase the number of remaining copies again.
            if let Some(meta) = core.history.get_mut(&bid) {
                let old_size = meta.nodes.len();
                meta.nodes
                    .retain(|node_name| node_name.contains(&next_hop_node_name));
                if old_size != meta.nodes.len() + 1 {
                    warn!(
                        "Removed {} from bid {} entry, duplicate entries, should be 1!",
                        meta.nodes.len(),
                        &bid
                    );
                }
                meta.remaining_copies += 1;
            }
        }
        RoutingNotifcation::IncomingBundle(bndl) => {
            core.handle_new_bundle(bndl.id());
        }
        RoutingNotifcation::IncomingBundleWithoutPreviousNode(bid, _node_name) => {
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
        for (_, p) in (*PEERS.lock()).iter() {
            let peer_node_id = p.eid.node_id().unwrap();
            if peer_node_id == core.local_node || meta.nodes.contains(&peer_node_id) {
                // skip if the peer is ourself or if we already sent the bundle to this peer
                continue;
            }
            if meta.remaining_copies < 2 {
                // we are done with this bundle, only direct delivery remains
                if bp.destination.node().unwrap() == p.node_name() {
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
                meta.nodes.push(peer_node_id.clone());
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
        reply.send((clas, delete_afterwards)).unwrap();
    });
}
async fn handle_routing_cmd(mut rx: mpsc::Receiver<RoutingCmd>) {
    let settings = CONFIG.lock().routing_settings.clone();

    let max_copies = if let Some(settings) = settings.get("sprayandwait") {
        settings
            .get("num_copies")
            .unwrap_or(&format!("{}", MAX_COPIES))
            .parse::<usize>()
            .unwrap()
    } else {
        MAX_COPIES
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
            super::RoutingCmd::Notify(notification) => {
                handle_notification(&mut core, notification);
            }
        }
    }
}
