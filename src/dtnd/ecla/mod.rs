use async_trait::async_trait;
use bp7::EndpointID;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};

use tcp::TCPTransportLayer;
use ws::WebsocketTransportLayer;

pub mod processing;
pub mod tcp;
pub mod ws;

mod base64 {
    use base64::{decode, encode};
    use serde::{Deserialize, Serialize};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let base64 = encode(v);
        String::serialize(&base64, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        decode(base64.as_bytes()).map_err(|e| serde::de::Error::custom(e))
    }
}

// The variant of Packets that can be send or received. The resulting JSON will have
// a field called type that encodes the selected variant.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Packet {
    RegisterPacket(RegisterPacket),
    Beacon(Beacon),
    ForwardDataPacket(ForwardDataPacket),
}

// Beacon is a device discovery packet. It can either be from the direct connection
// to the dtnd or received over the transmission layer of the ECLA client.
#[derive(Serialize, Deserialize)]
pub struct Beacon {
    eid: EndpointID,
    addr: String,
    #[serde(with = "base64")]
    service_block: Vec<u8>,
}

// Identification Packet that registers the Module Name.
#[derive(Serialize, Deserialize)]
pub struct RegisterPacket {
    name: String,
    enable_beacon: bool,
}

// Packet that forwards Bundle data
#[derive(Serialize, Deserialize)]
pub struct ForwardDataPacket {
    src: String,
    dst: String,
    #[serde(with = "base64")]
    data: Vec<u8>,
}

#[enum_dispatch]
pub enum TransportLayerEnum {
    WebsocketTransportLayer,
    TCPTransportLayer,
}

#[async_trait]
#[enum_dispatch(TransportLayerEnum)]
pub trait TransportLayer {
    async fn setup(&mut self);
    fn name(&self) -> &str;
    fn send_packet(&self, dest: &str, packet: &Packet) -> bool;
    fn close(&self, dest: &str);
}
