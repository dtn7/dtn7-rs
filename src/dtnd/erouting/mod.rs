use crate::cla::ClaSender;
use crate::{BundlePack, DtnPeer};
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
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendForBundlePacket {
    pub clas: Vec<String>,
    pub bp: BundlePack,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendForBundleResponsePacket {
    pub clas: Vec<ClaSender>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SendingFailedPacket {
    pub a: String,
    pub b: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundlePacket {
    pub bndl: Bundle,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IncomingBundleWithoutPreviousNodePacket {
    pub a: String,
    pub b: String,
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
