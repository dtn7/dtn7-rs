use super::{Packet, RegisterPacket};
use crate::dtnd::ecla::ws_client::Command::SendPacket;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures_util::{future, pin_mut, StreamExt};
use log::info;
use serde_json::Result;
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

pub enum Command {
    SendPacket(Packet),
    Close,
}

pub struct Client {
    module_name: String,
    enable_beacon: bool,
    ip: String,
    id: String,
    port: i16,
    cmd_receiver: UnboundedReceiver<Command>,
    cmd_sender: UnboundedSender<Command>,
    packet_out: UnboundedSender<Packet>,
}

pub fn new(
    module_name: &str,
    addr: &str,
    current_id: &str,
    packet_out: UnboundedSender<Packet>,
    enable_beacon: bool,
) -> std::io::Result<Client> {
    let parts: Vec<&str> = addr.split(':').collect();

    if parts.len() != 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "addr is not in format ip:port",
        ));
    }

    let (cmd_sender, cmd_receiver) = unbounded::<Command>();

    Ok(Client {
        module_name: module_name.to_string(),
        ip: parts[0].to_string(),
        id: current_id.to_string(),
        port: i16::from_str(parts[1]).expect("could not parse port"),
        enable_beacon,
        cmd_receiver,
        cmd_sender,
        packet_out,
    })
}

impl Client {
    pub async fn connect(&mut self) -> Result<()> {
        let (ws_stream, _) = connect_async(format!("ws://{}:{}/ws/ecla", self.ip, self.port))
            .await
            .expect("Failed to connect");

        info!("WebSocket handshake has been successfully completed");

        let (write, read) = ws_stream.split();

        // Queue initial RegisterPacket
        self.cmd_sender
            .unbounded_send(SendPacket(Packet::RegisterPacket(RegisterPacket {
                name: self.module_name.to_string(),
                enable_beacon: self.enable_beacon,
            })))
            .expect("couldn't send RegisterPacket");

        // Pass rx to write
        let cmd_receiver = std::mem::replace(&mut self.cmd_receiver, unbounded().1);
        let to_ws = cmd_receiver
            .filter_map(|command| async {
                match command {
                    Command::SendPacket(packet) => {
                        let data = serde_json::to_string(&packet);
                        return Some(Message::Text(data.unwrap()));
                    }
                    Command::Close => {}
                }
                None
            })
            .map(Ok)
            .forward(write);

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
                            self.packet_out
                                .unbounded_send(Packet::ForwardDataPacket(fwd))
                                .expect("couldn't send ForwardDataPacket");
                        }
                        Packet::Beacon(mut pdp) => {
                            pdp.addr = self.id.clone();
                            self.packet_out
                                .unbounded_send(Packet::Beacon(pdp))
                                .expect("couldn't send Beacon");
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

    pub fn command_channel(&self) -> UnboundedSender<Command> {
        self.cmd_sender.clone()
    }
}
