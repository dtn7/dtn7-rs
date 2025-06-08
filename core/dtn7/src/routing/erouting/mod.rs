use crate::{peers_get_for_node, BundleID, BundlePack, DtnPeer, PeerAddress, RoutingNotifcation};
use bp7::{Bundle, EndpointID};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

pub mod processing;
pub mod ws_client;

/*

    The External Routing allows implementing routing algorithms externally (e.g. outside the dtn7-rs codebase).
    It works by exposing a realtime JSON API via WebSocket. With the help of the erouting it is possible to easily
    implement new routing algorithms in different languages. All languages that can encode / decode JSON
    and communicate via WebSocket should in theory work.

*/

/// The variant of Packets that can be sent or received. The resulting JSON will have
/// a field called type that encodes the selected variant.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Packet {
    /// Packet that contains information about a bundle that should be sent.
    RequestSenderForBundle(RequestSenderForBundle),
    /// Packet response to a RequestSenderForBundle packet. Contains the original
    /// bundle pack and a list of senders where the packet should be forwarded to.
    ResponseSenderForBundle(ResponseSenderForBundle),
    /// Packet that contains information about an occurred error.
    Error(Error),
    /// If no ResponseSenderForBundle was received in a certain timeframe a
    /// Timeout packet will be emitted.
    Timeout(Timeout),
    /// Packet that signals that the sending failed.
    SendingFailed(SendingFailed),
    /// Packet that signals that the sending succeeded.
    SendingSucceeded(SendingSucceeded),
    /// Packet that signals that a bundle is incoming.
    IncomingBundle(IncomingBundle),
    /// Packet that signals that a bundle is incoming without a previous node.
    IncomingBundleWithoutPreviousNode(IncomingBundleWithoutPreviousNode),
    /// Packet that signals that a new peer was encountered.
    EncounteredPeer(EncounteredPeer),
    /// Packet that signals that a new peer was dropped.
    DroppedPeer(DroppedPeer),
    /// Packet that contains the full initial peer state of dtnd at the point of connection.
    PeerState(PeerState),
    /// Packet that creates a new service in dtnd.
    ServiceState(ServiceState),
    /// Packet that contains the full initial service state of dtnd at the point of connection.
    ServiceAdd(AddService),
}

impl From<RoutingNotifcation> for Packet {
    fn from(notification: RoutingNotifcation) -> Self {
        match notification {
            RoutingNotifcation::SendingFailed(bid, cla_sender) => {
                Packet::SendingFailed(SendingFailed { bid, cla_sender })
            }
            RoutingNotifcation::SendingSucceeded(bid, cla_sender) => {
                Packet::SendingSucceeded(SendingSucceeded { bid, cla_sender })
            }
            RoutingNotifcation::IncomingBundle(bndl) => {
                Packet::IncomingBundle(IncomingBundle { bndl })
            }
            RoutingNotifcation::IncomingBundleWithoutPreviousNode(bid, node_name) => {
                Packet::IncomingBundleWithoutPreviousNode(IncomingBundleWithoutPreviousNode {
                    bid,
                    node_name,
                })
            }
            RoutingNotifcation::EncounteredPeer(eid) => Packet::EncounteredPeer(EncounteredPeer {
                name: eid.node().unwrap(),
                eid: eid.clone(),
                peer: peers_get_for_node(&eid).unwrap(),
            }),
            RoutingNotifcation::DroppedPeer(eid) => Packet::DroppedPeer(DroppedPeer {
                name: eid.node().unwrap(),
                eid,
            }),
        }
    }
}

/// Sender is a selected sender for bundle delivery.
#[derive(Serialize, Deserialize, Clone)]
pub struct Sender {
    pub remote: PeerAddress,
    /// If the remote is a IP address and requires a port.
    pub port: Option<u16>,
    pub agent: String,
    pub next_hop: EndpointID,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RequestSenderForBundle {
    pub clas: Vec<String>,
    pub bp: BundlePack,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Error {
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ResponseSenderForBundle {
    pub bp: BundlePack,
    pub clas: Vec<Sender>,
    pub delete_afterwards: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Timeout {
    pub bp: BundlePack,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendingFailed {
    pub bid: BundleID,
    pub cla_sender: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendingSucceeded {
    pub bid: BundleID,
    pub cla_sender: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundle {
    pub bndl: Bundle,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundleWithoutPreviousNode {
    pub bid: BundleID,
    pub node_name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EncounteredPeer {
    pub name: String,
    pub eid: EndpointID,
    pub peer: DtnPeer,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DroppedPeer {
    pub name: String,
    pub eid: EndpointID,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PeerState {
    pub peers: BTreeMap<String, DtnPeer>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AddService {
    pub tag: u8,
    pub service: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ServiceState {
    pub service_list: HashMap<u8, String>,
}
