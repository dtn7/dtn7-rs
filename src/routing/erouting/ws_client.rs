use super::Packet;
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures_util::{future, pin_mut, SinkExt, StreamExt};
use log::info;
use serde_json::Result;
use std::str::FromStr;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub enum Command {
    SendPacket(Packet),
    Close,
}

pub struct Client {
    ip: String,
    port: i16,
    cmd_receiver: UnboundedReceiver<Command>,
    cmd_sender: UnboundedSender<Command>,
    packet_out: UnboundedSender<Packet>,
}

pub fn new(addr: &str, packet_out: UnboundedSender<Packet>) -> std::io::Result<Client> {
    let parts: Vec<&str> = addr.split(':').collect();

    if parts.len() != 2 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "addr is not in format ip:port",
        ));
    }

    let (cmd_sender, cmd_receiver) = unbounded::<Command>();

    Ok(Client {
        ip: parts[0].to_string(),
        port: i16::from_str(parts[1]).expect("could not parse port"),
        cmd_receiver,
        cmd_sender,
        packet_out,
    })
}

impl Client {
    pub async fn connect(&mut self) -> Result<()> {
        let (ws_stream, _) = connect_async(format!("ws://{}:{}/ws/erouting", self.ip, self.port))
            .await
            .expect("Failed to connect");

        info!("WebSocket handshake has been successfully completed");

        let (mut write, read) = ws_stream.split();

        // Pass rx to write
        let mut cmd_receiver = std::mem::replace(&mut self.cmd_receiver, unbounded().1);
        let to_ws = tokio::spawn(async move {
            while let Some(command) = cmd_receiver.next().await {
                match command {
                    Command::SendPacket(packet) => {
                        let data = serde_json::to_string(&packet);
                        write
                            .send(Message::Text(data.unwrap()))
                            .await
                            .expect("couldn't send packet");
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
                let data = message.unwrap().into_text();

                let packet: Result<Packet> = serde_json::from_str(data.unwrap().as_str());
                if let Ok(packet) = packet {
                    self.packet_out
                        .unbounded_send(packet)
                        .expect("could't send packet");
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
