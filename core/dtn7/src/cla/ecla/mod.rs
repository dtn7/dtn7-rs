use async_trait::async_trait;
use bp7::EndpointID;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

use tcp::TCPConnector;
use ws::WebsocketConnector;

pub mod processing;
pub mod tcp;
pub mod ws;
pub mod ws_client;

/*

    The External Convergence Layer Agent allows implementing Convergence Layer Agents externally (e.g. outside the dtn7-rs codebase).
    It works by exposing a realtime JSON API via WebSocket or TCP. With the help of the ECLA it is possible to easily implement new transmission
    layers in different languages. All languages that can encode / decode JSON and communicate via WebSocket or TCP should in theory work.
    Additionally, the ECLA contains a optional and simple beacon system that can be used for peer discovery.

    A client that connects to the ECLA and implements a new transmission layer is called a External Convergence Layer Module (in short ECL-Module).

*/

mod base64 {
    use base64::{decode, encode};
    use serde::{Deserialize, Serialize};
    use serde::{Deserializer, Serializer};

    // TODO: Uses a extra allocation at the moment. Might be worth investigating a allocation-less solution in the future.

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let base64 = encode(v);
        String::serialize(&base64, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        decode(base64.as_bytes()).map_err(serde::de::Error::custom)
    }
}

/// The variant of Packets that can be sent or received. The resulting JSON will have
/// a field called type that encodes the selected variant.
#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Packet {
    /// Identification Packet that registers the Module with name and options.
    Register(Register),
    /// Beacon is a device discovery packet. This packet will either be send from
    /// dtnd to the ECLA Modules to advertise itself or received from a ECLA Module,
    /// containing a new discovered peer from the transmission layer.
    Beacon(Beacon),
    /// Packet that forwards Bundle data.
    ForwardData(ForwardData),
    /// Packet that contains information about the connected node (will be send if registration was successful).
    Registered(Registered),
    /// Packet that contains a error message if a error happens.
    Error(Error),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Beacon {
    pub eid: EndpointID,
    /// Some addressable id in the transportation layer (e.g. IP Address, Bluetooth MAC, ...)
    pub addr: String,
    #[serde(with = "base64")]
    pub service_block: Vec<u8>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Registered {
    pub eid: EndpointID,
    pub nodeid: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Error {
    pub reason: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Register {
    /// The name should refer to the type of transportation layer used in the ECLA (e.g. MTCP, LoRa, BLE, ...)
    pub name: String,
    /// Enables the optional neighborhood discovery. If enabled beacon packets will be periodically sent to the
    /// ECLA Module. Detailed information can be found in the ECLA docs (Found under '/doc/ecla.md' in repository).
    pub enable_beacon: bool,
    /// If the ECLA uses some kind of IP and port based protocol it needs to be known so that dtnd can use
    /// the port in the destination format (<addr>:<port>) generation in the peers.
    ///
    /// See [DtnPeer.first_cla()](crate::DtnPeer#method.first_cla) method source for information.
    ///
    /// Example: For mtcp this would be the listening port on which it accepts connections and data.
    pub port: Option<u16>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ForwardData {
    /// Some addressable id of the source in the transportation layer (e.g. IP Address, Bluetooth MAC, ...).
    /// The content is free to choose and it's the responsibility of the ECLA to interpret them.
    pub src: String,
    /// Some addressable id of the destination in the transportation layer (e.g. IP Address, Bluetooth MAC, ...).
    /// The content is free to choose and it's the responsibility of the ECLA to interpret them.
    pub dst: String,
    pub bundle_id: String,
    #[serde(with = "base64")]
    pub data: Vec<u8>,
}

/// Connection represents the session of a connection with a Tx channel to send data
/// and a oneshot channel to signal a closing of the session once. Can be used as generic
/// session for connectors.
struct Connection<A> {
    tx: Sender<A>,
    close: Option<oneshot::Sender<()>>,
}

#[enum_dispatch]
pub enum ConnectorEnum {
    WebsocketConnector,
    TCPConnector,
}

#[async_trait]
#[enum_dispatch(ConnectorEnum)]
/// Trait to implement transport connector (e.g. WebSocket, TCP, ...) over which ecla modules can connect to.
pub trait Connector {
    async fn setup(&mut self);
    fn name(&self) -> &str;
    fn send_packet(&self, dest: &str, packet: &Packet) -> bool;
    fn close(&self, dest: &str);
}
