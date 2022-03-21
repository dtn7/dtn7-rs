use crate::{BundlePack, DtnPeer, PeerAddress};
use bp7::{Bundle, EndpointID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod processing;
pub mod ws_client;

/*

    The External Routing allows implementing routing algorithms externally (e.g. outside the dtn7-rs codebase).
    It works by exposing a realtime JSON API via WebSocket. With the help of the erouting it is possible to easily
    implement new routing algorithms in different language. All languages that can encode / decode JSON
    and communicate via WebSocket should in theory work.

*/

/// The variant of Packets that can be send or received. The resulting JSON will have
/// a field called type that encodes the selected variant.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Packet {
    SenderForBundle(SenderForBundle),
    SenderForBundleResponse(SenderForBundleResponse),
    Timeout(Timeout),
    SendingFailed(SendingFailed),
    IncomingBundle(IncomingBundle),
    IncomingBundleWithoutPreviousNode(IncomingBundleWithoutPreviousNode),
    EncounteredPeer(EncounteredPeer),
    DroppedPeer(DroppedPeer),
    PeerState(PeerState),
    ServiceState(ServiceState),
    ServiceAdd(AddService),
}

/// Packet that contains information about a bundle that should be send.
#[derive(Serialize, Deserialize, Clone)]
pub struct SenderForBundle {
    pub clas: Vec<String>,
    pub bp: BundlePack,
}

/// Sender is a selected sender for bundle delivery.
#[derive(Serialize, Deserialize, Clone)]
pub struct Sender {
    pub remote: PeerAddress,
    pub port: Option<u16>,
    pub agent: String,
    pub next_hop: EndpointID,
}

/// Packet response to a SenderForBundle packet. Contains the original
/// bundle pack and a list of senders where the packet should be forwarded to.
#[derive(Serialize, Deserialize, Clone)]
pub struct SenderForBundleResponse {
    pub bp: BundlePack,
    pub clas: Vec<Sender>,
    pub delete_afterwards: bool,
}

/// If no SenderForBundleResponse was received in a certain timeframe a
/// Timeout packet will be emitted.
#[derive(Serialize, Deserialize, Clone)]
pub struct Timeout {
    pub bp: BundlePack,
}

/// Packet that signals that the sending failed.
#[derive(Serialize, Deserialize, Clone)]
pub struct SendingFailed {
    pub bid: String,
    pub cla_sender: String,
}

/// Packet that signals that a bundle is incoming.
#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundle {
    pub bndl: Bundle,
}

/// Packet that signals that a bundle is incoming without a previous node.
#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundleWithoutPreviousNode {
    pub bid: String,
    pub node_name: String,
}

/// Packet that signals that a new peer was encountered.
#[derive(Serialize, Deserialize, Clone)]
pub struct EncounteredPeer {
    pub name: String,
    pub eid: EndpointID,
    pub peer: DtnPeer,
}

/// Packet that signals that a new peer was dropped.
#[derive(Serialize, Deserialize, Clone)]
pub struct DroppedPeer {
    pub name: String,
    pub eid: EndpointID,
}

/// Packet that contains the full initial peer state of dtnd at the point of connection.
#[derive(Serialize, Deserialize, Clone)]
pub struct PeerState {
    pub peers: HashMap<String, DtnPeer>,
}

/// Packet that creates a new service in dtnd.
#[derive(Serialize, Deserialize, Clone)]
pub struct AddService {
    pub tag: u8,
    pub service: String,
}

/// Packet that contains the full initial service state of dtnd at the point of connection.
#[derive(Serialize, Deserialize, Clone)]
pub struct ServiceState {
    pub service_list: HashMap<u8, String>,
}
