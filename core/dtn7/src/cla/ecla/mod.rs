use async_trait::async_trait;
use bp7::EndpointID;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use tcp::TCPTransportLayer;
use ws::WebsocketTransportLayer;

pub mod processing;
pub mod tcp;
pub mod ws;
pub mod ws_client;

/*

    The External Convergence Layer Agent allows implementing Convergence Layer Agents externally (e.g. outside the dtn7-rs codebase).
    It works by exposing a realtime JSON API via WebSocket or TCP. With the help of the ECLA it is possible to easily implement new transmission
    layers in different language. All languages that can encode / decode JSON and communicate via WebSocket or TCP should in theory work.
    Additionally, the ECLA contains a optional and simple beacon system that can be used for peer discovery.

    A client that connects to the ECLA and implements a new transmission layer is called a External Convergence Layer Module (in short ECL-Module).

*/

mod base64 {
    use base64::{decode, encode};
    use serde::{Deserialize, Serialize};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let base64 = encode(v);
        String::serialize(&base64, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        decode(base64.as_bytes()).map_err(serde::de::Error::custom)
    }
}

/// The variant of Packets that can be send or received. The resulting JSON will have
/// a field called type that encodes the selected variant.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Packet {
    Register(Register),
    Beacon(Beacon),
    ForwardData(ForwardData),
    Registered(Registered),
    Error(Error),
}

/// Beacon is a device discovery packet. It can either be from the direct connection
/// to the dtnd or received over the transmission layer of the ECLA client.
#[derive(Serialize, Deserialize, Clone)]
pub struct Beacon {
    pub eid: EndpointID,
    pub addr: String,
    #[serde(with = "base64")]
    pub service_block: Vec<u8>,
}

/// Packet that contains information about the connected node (will be send if registration was successful).
#[derive(Serialize, Deserialize, Clone)]
pub struct Registered {
    pub eid: EndpointID,
    pub nodeid: String,
}

/// Packet that contains a error message if a error happens.
#[derive(Serialize, Deserialize, Clone)]
pub struct Error {
    pub reason: String,
}

/// Identification Packet that registers the Module Name.
#[derive(Serialize, Deserialize, Clone)]
pub struct Register {
    pub name: String,
    pub enable_beacon: bool,
    pub port: Option<u16>,
}

/// Packet that forwards Bundle data.
#[derive(Serialize, Deserialize, Clone)]
pub struct ForwardData {
    pub src: String,
    pub dst: String,
    pub bundle_id: String,
    #[serde(with = "base64")]
    pub data: Vec<u8>,
}

#[enum_dispatch]
pub enum TransportLayerEnum {
    WebsocketTransportLayer,
    TCPTransportLayer,
}

#[async_trait]
#[enum_dispatch(TransportLayerEnum)]
/// Trait to implement transport layer (e.g. WebSocket, TCP, ...) over which ecla modules can connect to.
pub trait TransportLayer {
    async fn setup(&mut self);
    fn name(&self) -> &str;
    fn send_packet(&self, dest: &str, packet: &Packet) -> bool;
    fn close(&self, dest: &str);
}
