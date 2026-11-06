use super::RoutingAgent;
use crate::cla::ClaSenderTask;
use crate::core::bundlepack::BundlePack;
use crate::routing::{RoutingCmd, RoutingNotification};
use crate::DtnPeer;
use crate::PEERS;
use async_trait::async_trait;
use log::{debug, trace};
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

/// Keeps history about which bundles were already sent to what peers.
struct EpidemicRoutingAgentCore {
    history: HashMap<String, HashSet<String>>,
}

impl EpidemicRoutingAgentCore {
    pub fn new() -> EpidemicRoutingAgentCore {
        EpidemicRoutingAgentCore {
            history: HashMap::new(),
        }
    }

    fn add(&mut self, bundle_id: String, node_name: String) {
        let entries = self.history.entry(bundle_id).or_default();
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

    fn contains(&self, bundle_id: &str, node_name: &str) -> bool {
        if let Some(entries) = self.history.get(bundle_id) {
            //let entries = self.history.entry(bundle_id);
            return entries.contains(node_name);
        }
        false
    }

    fn sending_failed(&mut self, bundle_id: &str, node_name: &str) {
        if let Some(entries) = self.history.get_mut(bundle_id) {
            entries.remove(node_name);
            debug!(
                "removed {:?} from sent list for bundle {}",
                node_name, bundle_id
            );
        }
    }

    fn incoming_bundle(&mut self, bundle_id: &str, node_name: &str) {
        if !node_name.is_empty() && !self.contains(bundle_id, node_name) {
            self.add(bundle_id.to_string(), node_name.to_string());
        }
    }

    /// Selects the peers that have not received the bundle yet, or only the destination
    /// peer if direct delivery is possible. Only the selected peers are recorded as sent.
    fn senders_for_bundle<'a>(
        &mut self,
        bp: &BundlePack,
        peers: impl IntoIterator<Item = &'a DtnPeer>,
    ) -> (Vec<ClaSenderTask>, bool) {
        let mut selected: Vec<(String, ClaSenderTask)> = Vec::new();
        let mut delete_afterwards = false;
        for p in peers {
            trace!("checking peer {:?} (node_name={})", p, p.node_name());
            if self.contains(bp.id(), &p.node_name()) {
                continue;
            }
            if let Some(cla) = p.first_cla() {
                if bp.destination.node().as_deref() == Some(p.node_name().as_str()) {
                    // direct delivery possible
                    debug!(
                        "Attempting direct delivery of bundle {} to {}",
                        bp.id(),
                        p.node_name()
                    );
                    delete_afterwards = true;
                    selected = vec![(p.node_name(), cla)];
                    break;
                }
                selected.push((p.node_name(), cla));
            }
        }
        let mut clas = Vec::with_capacity(selected.len());
        for (node_name, cla) in selected {
            self.add(bp.id().to_string(), node_name);
            clas.push(cla);
        }
        (clas, delete_afterwards)
    }
}

async fn handle_routing_cmd(mut rx: mpsc::Receiver<RoutingCmd>) {
    let mut core: EpidemicRoutingAgentCore = EpidemicRoutingAgentCore::new();

    while let Some(cmd) = rx.recv().await {
        match cmd {
            super::RoutingCmd::SenderForBundle(bp, reply) => {
                debug!("checking PEERS due to SenderForBundle request (bp={})", bp);
                let (clas, delete_afterwards) =
                    core.senders_for_bundle(&bp, (*PEERS.lock()).values());

                tokio::spawn(async move {
                    // the requester may already be gone (e.g., timed out), which is not an error here
                    let _ = reply.send((clas, delete_afterwards));
                });
            }
            super::RoutingCmd::Shutdown => {
                break;
            }
            super::RoutingCmd::Command(_cmd) => {}
            super::RoutingCmd::GetData(_, tx) => {
                // the requester may already be gone (e.g., timed out), which is not an error here
                let _ = tx.send(format!("{:?}", core.history));
            }
            super::RoutingCmd::Notify(notification) => match notification {
                RoutingNotification::SendingFailed(bid, cla_sender) => {
                    core.sending_failed(bid.as_str(), cla_sender.as_str());
                }
                RoutingNotification::IncomingBundle(bndl) => {
                    if let Some(eid) = bndl.previous_node() {
                        if let Some(node_name) = eid.node() {
                            core.incoming_bundle(&bndl.id(), &node_name);
                        }
                    };
                }
                RoutingNotification::IncomingBundleWithoutPreviousNode(bid, node_name) => {
                    core.incoming_bundle(bid.as_str(), node_name.as_str());
                }
                _ => {}
            },
        }
    }
}

impl EpidemicRoutingAgent {
    pub fn new() -> EpidemicRoutingAgent {
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            handle_routing_cmd(rx).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cla::dummy::DummyConvergenceLayer;
    use crate::core::peer::{PeerAddress, PeerType};
    use bp7::EndpointID;
    use std::convert::TryFrom;

    fn peer(name: &str) -> DtnPeer {
        DtnPeer::new(
            EndpointID::try_from(format!("dtn://{}/", name)).unwrap(),
            PeerAddress::Ip("127.0.0.1".parse().unwrap()),
            PeerType::Static,
            None,
            vec![("dummy".into(), None)],
            HashMap::new(),
        )
    }

    fn bundle_pack(dst: &str) -> BundlePack {
        let bndl = bp7::bundle::new_std_payload_bundle(
            EndpointID::try_from("dtn://epidemic-src/app".to_string()).unwrap(),
            EndpointID::try_from(dst.to_string()).unwrap(),
            b"epidemic".to_vec(),
        );
        BundlePack::from(&bndl)
    }

    #[tokio::test]
    async fn direct_delivery_only_records_destination() {
        crate::cla_add(DummyConvergenceLayer::new(None).into());
        let peers = [peer("relay1"), peer("dest"), peer("relay2")];
        let bp = bundle_pack("dtn://dest/inbox");

        let mut core = EpidemicRoutingAgentCore::new();
        let (clas, delete_afterwards) = core.senders_for_bundle(&bp, peers.iter());
        assert_eq!(clas.len(), 1);
        assert_eq!(clas[0].next_hop, peers[1].eid);
        assert!(delete_afterwards);
        assert!(core.contains(bp.id(), "dest"));
        assert!(!core.contains(bp.id(), "relay1"));
        assert!(!core.contains(bp.id(), "relay2"));
    }

    #[tokio::test]
    async fn relays_are_recorded_and_not_selected_again() {
        crate::cla_add(DummyConvergenceLayer::new(None).into());
        let peers = [peer("relay1"), peer("relay2")];
        let bp = bundle_pack("dtn://elsewhere/inbox");

        let mut core = EpidemicRoutingAgentCore::new();
        let (clas, delete_afterwards) = core.senders_for_bundle(&bp, peers.iter());
        assert_eq!(clas.len(), 2);
        assert!(!delete_afterwards);
        let (clas, _) = core.senders_for_bundle(&bp, peers.iter());
        assert!(clas.is_empty());
    }
}
