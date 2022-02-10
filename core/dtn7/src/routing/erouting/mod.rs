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
    SendForBundlePacket(SendForBundlePacket),
    SendForBundleResponsePacket(SendForBundleResponsePacket),
    SendingFailedPacket(SendingFailedPacket),
    IncomingBundlePacket(IncomingBundlePacket),
    IncomingBundleWithoutPreviousNodePacket(IncomingBundleWithoutPreviousNodePacket),
    EncounteredPeerPacket(EncounteredPeerPacket),
    DroppedPeerPacket(DroppedPeerPacket),
    PeerStatePacket(PeerStatePacket),
    ServiceStatePacket(ServiceStatePacket),
    ServiceAddPacket(AddServicePacket),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendForBundlePacket {
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
pub struct SendForBundleResponsePacket {
    pub bp: BundlePack,
    pub clas: Vec<Sender>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendingFailedPacket {
    pub bid: String,
    pub cla_sender: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundlePacket {
    pub bndl: Bundle,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundleWithoutPreviousNodePacket {
    pub bid: String,
    pub node_name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct EncounteredPeerPacket {
    pub eid: EndpointID,
    pub peer: DtnPeer,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DroppedPeerPacket {
    pub eid: EndpointID,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PeerStatePacket {
    pub peers: HashMap<String, DtnPeer>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AddServicePacket {
    pub tag: u8,
    pub service: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ServiceStatePacket {
    pub service_list: HashMap<u8, String>,
}
