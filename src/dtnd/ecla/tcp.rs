use super::TransportLayer;
use crate::dtnd::ecla::processing::{handle_connect, handle_disconnect, handle_packet};
use crate::dtnd::ecla::Packet;
use crate::lazy_static;
use async_trait::async_trait;
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{stream::TryStreamExt, StreamExt};
use log::debug;
use log::info;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::net::{TcpListener, TcpStream};
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};

struct Connection {
    tx: Tx,
}

type Tx = UnboundedSender<Vec<u8>>;
type PeerMap = Arc<Mutex<HashMap<String, Connection>>>;

lazy_static! {
    static ref PEER_MAP: PeerMap = PeerMap::new(Mutex::new(HashMap::new()));
}

// Handles the websocket connection.
async fn handle_connection(mut raw_stream: TcpStream, addr: SocketAddr) {
    info!("Incoming TCP connection from: {}", addr);

    let (tx, rx) = unbounded();

    // Insert the write part of this peer to the peer map.
    PEER_MAP
        .lock()
        .unwrap()
        .insert(addr.to_string(), Connection { tx });
    handle_connect("TCP".to_string(), addr.to_string());

    // Receiver task
    tokio::spawn(async move {
        let (incoming, _) = raw_stream.split();

        // Delimit frames using a length header
        let length_delimited = FramedRead::new(incoming, LengthDelimitedCodec::new());

        // Deserialize frames
        let mut deserialized = tokio_serde::SymmetricallyFramed::new(
            length_delimited,
            SymmetricalJson::<Packet>::default(),
        );

        while let res = deserialized.try_next().await {
            if res.is_err() {
                // TODO: check error
                break;
            }

            let packet = res.unwrap();
            if packet.is_some() {
                handle_packet("TCP".to_string(), addr.to_string(), packet.unwrap());
            } else {
                // TODO: is None => Disconnect?
                break;
            }
        }

        if PEER_MAP.lock().unwrap().remove(&addr.to_string()).is_some() {
            info!("{} disconnected", &addr);
            handle_disconnect(addr.to_string());
        }
    });

    // TODO: fix sending of packets

    // Sender task
    /*tokio::spawn(async move {
        while let Some(packet) = rx.try_next().await.unwrap() {
            outgoing.write(packet.as_slice());
        }
    });*/
}

#[derive(Clone, Default)]
pub struct TCPTransportLayer {
    port: u16,
}

impl TCPTransportLayer {
    pub fn new(port: u16) -> TCPTransportLayer {
        TCPTransportLayer { port }
    }
}

#[async_trait]
impl TransportLayer for TCPTransportLayer {
    async fn setup(&mut self) {
        let port = self.port;

        tokio::spawn(async move {
            let addr = String::from("127.0.0.1:") + port.to_string().as_str();

            // Create the event loop and TCP listener we'll accept connections on.
            let try_socket = TcpListener::bind(&addr).await;
            let listener = try_socket.expect("Failed to bind");
            info!("External Convergence Layer TCP Listening on: {}", addr);

            // Let's spawn the handling of each connection in a separate task.
            while let Ok((stream, addr)) = listener.accept().await {
                tokio::spawn(handle_connection(stream, addr));
            }
        });
    }

    fn name(&self) -> &str {
        return "TCP";
    }

    fn send_packet(&self, dest: &str, packet: &Packet) -> bool {
        debug!("Sending Packet to {} ({})", dest, self.name());

        let mut pmap = PEER_MAP.lock().unwrap();
        let target = pmap.get(dest);
        if target.is_some() {
            let mut data = serde_json::to_vec(&packet).unwrap();
            let len = (data.len() as i32).to_ne_bytes();
            data.splice(0..0, len.iter().cloned());

            target.unwrap().tx.unbounded_send(data);
            return true;
        }

        return false;
    }

    fn close(&self, dest: &str) {
        todo!()
    }
}
