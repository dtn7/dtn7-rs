use super::Packet;
use futures_util::{SinkExt, StreamExt, future, pin_mut};
use log::{error, info};
use serde_json::Result;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub enum Command {
    /// Requests a send of the given packet.
    SendPacket(Box<Packet>),
    /// Requests the shutdown of the client.
    Close,
}

/// Represents the client session of a external router.
pub struct Client {
    ip: String,
    port: u16,
    cmd_receiver: mpsc::Receiver<Command>,
    cmd_sender: mpsc::Sender<Command>,
    packet_out: mpsc::Sender<Packet>,
}

/// Creates a new extern router client.
///
/// # Arguments
///
/// * `addr` - Address to connect to in format ip:port without any websocket url parts.
/// * `packet_our` - Channel to which received packets will be passed
///
pub fn new(addr: &str, packet_out: mpsc::Sender<Packet>) -> std::io::Result<Client> {
    let parts: Vec<&str> = addr.split(':').collect();

    if parts.len() != 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "addr is not in format ip:port",
        ));
    }

    let (cmd_sender, cmd_receiver) = mpsc::channel(100);

    Ok(Client {
        ip: parts[0].to_string(),
        port: u16::from_str(parts[1]).expect("could not parse port"),
        cmd_receiver,
        cmd_sender,
        packet_out,
    })
}

impl Client {
    /// Connects and starts to handle packets. Will block until a severe error is encountered or the client is closed.
    pub async fn serve(&mut self) -> anyhow::Result<()> {
        let (ws_stream, _) =
            connect_async(format!("ws://{}:{}/ws/erouting", self.ip, self.port)).await?;

        info!("WebSocket handshake has been successfully completed");

        let (mut write, read) = ws_stream.split();

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

        // Read from websocket
        let from_ws = {
            read.for_each(|message| async {
                if message.is_err() {
                    // TODO: good way to handle it?
                    return;
                }

                let data = message.unwrap().into_text();

                let packet: Result<Packet> = serde_json::from_str(data.unwrap().as_str());
                if let Ok(packet) = packet
                    && let Err(err) = self.packet_out.send(packet).await
                {
                    error!("Error while sending packet to channel: {}", err);
                }
            })
        };

        // from_ws uses the for_each method that requires it to be pinned to
        // the stacked in order to work.
        pin_mut!(from_ws);
        future::select(to_ws, from_ws).await;

        Ok(())
    }

    pub fn command_channel(&self) -> mpsc::Sender<Command> {
        self.cmd_sender.clone()
    }
}
