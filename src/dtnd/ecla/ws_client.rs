use super::{Beacon, ForwardDataPacket, Packet, RegisterPacket};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures_util::{future, pin_mut, StreamExt};
use log::{error, info};
use serde_json::Result;
use std::convert::TryInto;
use std::fmt::format;
use std::str::FromStr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

type ForwardDataRequest = fn(packet: &ForwardDataPacket);
type BeaconRequest = fn(packet: &Beacon);

pub struct Client {
    module_name: String,
    ip: String,
    id: String,
    port: i16,
    to_tx: Option<UnboundedSender<Message>>,
    from_tx: UnboundedSender<Packet>,
}

pub fn new(
    module_name: &str,
    addr: &str,
    current_id: &str,
    tx: UnboundedSender<Packet>,
) -> std::io::Result<Client> {
    let parts: Vec<&str> = addr.split(":").collect();

    if parts.len() != 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "addr is not in format ip:port",
        )
        .into());
    }

    return Ok(Client {
        module_name: module_name.to_string(),
        ip: parts[0].to_string(),
        id: current_id.to_string(),
        port: i16::from_str(parts[1]).expect("could not parse port"),
        from_tx: tx,
        to_tx: None,
    });
}

impl Client {
    pub async fn connect(&mut self) -> Result<()> {
        let (ws_stream, _) = connect_async(format!("ws://{}:{}", self.ip, self.port))
            .await
            .expect("Failed to connect");

        info!("WebSocket handshake has been successfully completed");

        let (write, read) = ws_stream.split();
        let (to_tx, to_rx) = unbounded::<Message>();

        // Send initial RegisterPacket
        let data = serde_json::to_string(&Packet::RegisterPacket(RegisterPacket {
            name: self.module_name.to_string(),
            enable_beacon: true,
        }));
        to_tx.unbounded_send(Message::Text(data.unwrap()));

        self.to_tx = Some(to_tx);

        // Pass rx to write
        let to_ws = to_rx.map(Ok).forward(write);

        // Read from websocket
        let from_ws = {
            read.for_each(|message| async {
                let data = message.unwrap().into_text();

                let packet: Result<Packet> = serde_json::from_str(data.unwrap().as_str());
                if let Ok(packet) = packet {
                    // Pass received packets to read channel
                    match packet {
                        Packet::ForwardDataPacket(mut fwd) => {
                            fwd.src = self.id.clone();
                            self.from_tx.unbounded_send(Packet::ForwardDataPacket(fwd));
                        }
                        Packet::Beacon(mut pdp) => {
                            pdp.addr = self.id.clone();
                            self.from_tx.unbounded_send(Packet::Beacon(pdp));
                        }
                        _ => {}
                    }
                }
            })
        };

        pin_mut!(to_ws, from_ws);
        future::select(to_ws, from_ws).await;

        Ok(())
    }

    pub fn insert_forward_data(&mut self, fwd: ForwardDataPacket) {
        if let Some(to_tx) = self.to_tx.as_ref() {
            let data = serde_json::to_string(&Packet::ForwardDataPacket(fwd));
            to_tx.unbounded_send(Message::Text(data.unwrap()));
        }
    }

    pub fn insert_beacon(&mut self, b: Beacon) {
        if let Some(to_tx) = self.to_tx.as_ref() {
            let data = serde_json::to_string(&Packet::Beacon(b));
            to_tx.unbounded_send(Message::Text(data.unwrap()));
        }
    }
}
