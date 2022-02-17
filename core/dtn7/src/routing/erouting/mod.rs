use crate::{BundlePack, DtnPeer, PeerAddress};
use bp7::{Bundle, EndpointID};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod processing;
pub mod ws_client;

// The variant of Packets that can be send or received. The resulting JSON will have
// a field called type that encodes the selected variant.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Packet {
    SendForBundle(SendForBundle),
    SendForBundleResponse(SendForBundleResponse),
    SendingFailed(SendingFailed),
    IncomingBundle(IncomingBundle),
    IncomingBundleWithoutPreviousNode(IncomingBundleWithoutPreviousNode),
    EncounteredPeer(EncounteredPeer),
    DroppedPeer(DroppedPeer),
    PeerState(PeerState),
    ServiceState(ServiceState),
    ServiceAdd(AddService),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendForBundle {
    pub clas: Vec<String>,
    pub bp: BundlePack,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Sender {
    pub remote: PeerAddress,
    pub port: Option<u16>,
    pub agent: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendForBundleResponse {
    pub bp: BundlePack,
    pub clas: Vec<Sender>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendingFailed {
    pub bid: String,
    pub cla_sender: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundle {
    pub bndl: Bundle,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundleWithoutPreviousNode {
    pub bid: String,
    pub node_name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EncounteredPeer {
    pub eid: EndpointID,
    pub peer: DtnPeer,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DroppedPeer {
    pub eid: EndpointID,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PeerState {
    pub peers: HashMap<String, DtnPeer>,
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
