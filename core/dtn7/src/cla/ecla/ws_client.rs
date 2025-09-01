use super::{Packet, Register};
use crate::cla::ecla;
use crate::cla::ecla::ws_client::Command::SendPacket;
use anyhow::bail;
use futures::channel::mpsc::unbounded;
use futures_util::{future, pin_mut, SinkExt, StreamExt};
use log::{error, info, warn};
use serde_json::Result;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

pub enum Command {
    /// Requests a send of the given packet.
    SendPacket(Packet),
    /// Requests the shutdown of the client.
    Close,
}

/// Represents the client session of a ecla module.
pub struct Client {
    module_name: String,
    enable_beacon: bool,
    ip: String,
    id: String,
    port: u16,
    ecla_port: Option<u16>,
    cmd_receiver: mpsc::Receiver<Command>,
    cmd_sender: mpsc::Sender<Command>,
    packet_out: mpsc::Sender<Packet>,
}

/// Creates a new ecla client.
///
/// # Arguments
///
/// * `module_name` - The name of the ecla module (most likely name of the transportation layer)
/// * `addr` - Address to connect to in format ip:port without any websocket url parts.
/// * `current_id` - Addressable id (e.g. IP, BL MAC, ...) of the transport layer (optional)
/// * `packet_our` - Channel to which received packets will be passed
/// * `enable_beacon` - If the optional service discovery should be enabled and beacon packets received.
///
pub fn new(
    module_name: &str,
    addr: &str,
    current_id: &str,
    packet_out: mpsc::Sender<Packet>,
    enable_beacon: bool,
) -> std::io::Result<Client> {
    let parts: Vec<&str> = addr.split(':').collect();

    if parts.len() != 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "addr is not in format ip:port",
        ));
    }

    let (cmd_sender, cmd_receiver) = mpsc::channel(100);

    let port = u16::from_str(parts[1]);
    if port.is_err() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "couldn't parse port",
        ));
    }

    Ok(Client {
        module_name: module_name.to_string(),
        ip: parts[0].to_string(),
        id: current_id.to_string(),
        port: port.unwrap(),
        ecla_port: None,
        enable_beacon,
        cmd_receiver,
        cmd_sender,
        packet_out,
    })
}

impl Client {
    /// Connects and starts to handle packets. Will block until a severe error is encountered or the client is closed.
    pub async fn serve(&mut self) -> anyhow::Result<()> {
        let (ws_stream, _) =
            connect_async(format!("ws://{}:{}/ws/ecla", self.ip, self.port)).await?;

        info!("WebSocket handshake has been successfully completed");

        let (mut write, read) = ws_stream.split();

        // Queue initial RegisterPacket
        if let Err(err) = self
            .cmd_sender
            .send(SendPacket(Packet::Register(Register {
                name: self.module_name.to_string(),
                enable_beacon: self.enable_beacon,
                port: self.ecla_port,
            })))
            .await
        {
            bail!("error while sending registration packet: {}", err);
        }

        // Pass rx to write
        let mut cmd_receiver = std::mem::replace(&mut self.cmd_receiver, mpsc::channel(1).1);
        let to_ws = tokio::spawn(async move {
            while let Some(command) = cmd_receiver.recv().await {
                match command {
                    Command::SendPacket(packet) => {
                        let data = serde_json::to_string(&packet).unwrap();
                        if write.send(Message::Text(data.into())).await.is_err() {
                            error!("Error while sending packet");
                        }
                    }
                    Command::Close => {
                        break;
                    }
                }
            }
        });

        let (err_sender, err_receiver) = unbounded::<ecla::Error>();

        // Read from websocket
        let from_ws = {
            read.for_each(|message| async {
                if message.is_err() {
                    return;
                }

                let data = message.unwrap().into_text();

                let packet: Result<Packet> = serde_json::from_str(data.unwrap().as_str());
                if let Ok(packet) = packet {
                    // Pass received packets to read channel
                    match packet {
                        Packet::ForwardData(mut fwd) => {
                            fwd.src.clone_from(&self.id);

                            if let Err(err) = self.packet_out.send(Packet::ForwardData(fwd)).await {
                                error!("Error while sending ForwardData to channel: {}", err);
                            }
                        }
                        Packet::Registered(reg) => {
                            if let Err(err) = self.packet_out.send(Packet::Registered(reg)).await {
                                error!("Error while sending Registered to channel: {}", err);
                            }
                        }
                        Packet::Beacon(mut pdp) => {
                            pdp.addr.clone_from(&self.id);

                            if let Err(err) = self.packet_out.send(Packet::Beacon(pdp)).await {
                                error!("Error while sending ForwardData to channel: {}", err);
                            }
                        }
                        Packet::Error(err) => {
                            info!("Error received: {}", err.reason);

                            if let Err(err) = err_sender.clone().send(err).await {
                                error!("Error while sending Error to channel: {}", err);
                            }
                        }
                        _ => {
                            warn!("Unexpected packet received!")
                        }
                    }
                }
            })
        };

        // from_ws and err_receiver use methods (for_each, try_next) that require to be pinned to
        // the stacked in order to work.
        pin_mut!(from_ws, err_receiver);
        future::select(to_ws, from_ws).await;

        if let Ok(Some(err)) = err_receiver.try_next() {
            bail!("{}", err.reason);
        }

        Ok(())
    }

    pub fn set_ecla_port(&mut self, port: u16) {
        self.ecla_port = Some(port);
    }

    pub fn set_current_id(&mut self, id: &str) {
        self.id = id.to_string();
    }

    pub fn command_channel(&self) -> mpsc::Sender<Command> {
        self.cmd_sender.clone()
    }
}
