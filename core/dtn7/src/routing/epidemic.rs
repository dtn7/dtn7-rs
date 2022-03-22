use super::RoutingAgent;
use crate::routing::{RoutingCmd, RoutingNotifcation};
use crate::PEERS;
use async_trait::async_trait;
use log::debug;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

/// Simple epidemic routing.
/// All bundles are sent to all known peers once via all CLAs.
#[derive(Debug)]
pub struct EpidemicRoutingAgent {
    tx: mpsc::Sender<super::RoutingCmd>,
}

impl Default for EpidemicRoutingAgent {
    fn default() -> Self {
        EpidemicRoutingAgent::new()
    }
}

fn add(history: &mut HashMap<String, HashSet<String>>, bundle_id: String, node_name: String) {
    let entries = history.entry(bundle_id).or_insert_with(HashSet::new);
    entries.insert(node_name);
}

/*fn remove_bundle(&mut self, bundle_id: String) {
    self.history.remove(&bundle_id);
}*/

/*fn filtered(&mut self, dest: String, bundles: &[ByteBuffer]) -> Vec<ByteBuffer> {
    let entries = self.history.entry(dest).or_insert_with(HashSet::new);
    bundles
        .iter()
        .cloned()
        .filter(|b| !entries.contains(b))
        .collect()
}*/

fn contains(
    history: &mut HashMap<String, HashSet<String>>,
    bundle_id: &str,
    node_name: &str,
) -> bool {
    if let Some(entries) = history.get(bundle_id) {
        //let entries = self.history.entry(bundle_id);
        return entries.contains(node_name);
    }
    false
}

fn sending_failed(
    history: &mut HashMap<String, HashSet<String>>,
    bundle_id: &str,
    node_name: &str,
) {
    if let Some(entries) = history.get_mut(bundle_id) {
        entries.remove(node_name);
        debug!(
            "removed {:?} from sent list for bundle {}",
            node_name, bundle_id
        );
    }
}

fn incoming_bundle(
    history: &mut HashMap<String, HashSet<String>>,
    bundle_id: &str,
    node_name: &str,
) {
    if !node_name.is_empty() && !contains(history, bundle_id, node_name) {
        add(history, bundle_id.to_string(), node_name.to_string());
    }
}

impl EpidemicRoutingAgent {
    pub fn new() -> EpidemicRoutingAgent {
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let mut history: HashMap<String, HashSet<String>> = HashMap::new();

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::RoutingCmd::SenderForBundle(bp, reply) => {
                        let mut clas = Vec::new();
                        let mut delete_afterwards = false;
                        for (_, p) in (*PEERS.lock()).iter() {
                            if !contains(&mut history, bp.id(), &p.node_name()) {
                                if let Some(cla) = p.first_cla() {
                                    add(&mut history, bp.id().to_string(), p.node_name().clone());
                                    if bp.destination.node().unwrap() == p.node_name() {
                                        // direct delivery possible
                                        debug!(
                                            "Attempting direct delivery of bundle {} to {}",
                                            bp.id(),
                                            p.node_name()
                                        );
                                        delete_afterwards = true;
                                        clas.clear();
                                        clas.push(cla);
                                        break;
                                    } else {
                                        clas.push(cla);
                                    }
                                }
                            }
                        }

                        tokio::spawn(async move {
                            reply.send((clas, delete_afterwards)).unwrap();
                        });
                    }
                    super::RoutingCmd::Shutdown => {
                        break;
                    }
                    super::RoutingCmd::Notify(notification) => match notification {
                        RoutingNotifcation::SendingFailed(bid, cla_sender) => {
                            sending_failed(&mut history, bid.as_str(), cla_sender.as_str());
                        }
                        RoutingNotifcation::IncomingBundle(bndl) => {
                            if let Some(eid) = bndl.previous_node() {
                                if let Some(node_name) = eid.node() {
                                    incoming_bundle(&mut history, &bndl.id(), &node_name);
                                }
                            };
                        }
                        RoutingNotifcation::IncomingBundleWithoutPreviousNode(bid, node_name) => {
                            incoming_bundle(&mut history, bid.as_str(), node_name.as_str());
                        }
                        _ => {}
                    },
                }
            }
        });

        EpidemicRoutingAgent { tx }
    }
}

impl std::fmt::Display for EpidemicRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "EpidemicRoutingAgent")
    }
}

#[async_trait]
impl RoutingAgent for EpidemicRoutingAgent {
    fn channel(&self) -> Sender<RoutingCmd> {
        self.tx.clone()
    }
}
