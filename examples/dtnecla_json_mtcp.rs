use anyhow::Result;
use clap::{crate_authors, crate_version, Arg, Command};
use clap::{value_parser, ArgAction};
use dtn7::client::ecla::{ws_client, Command::SendPacket, Packet};
use futures_util::{future, pin_mut, TryStreamExt};
use lazy_static::lazy_static;
use log::{debug, error, info};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddrV4;
use std::net::TcpStream;
use tokio::io;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, LengthDelimitedCodec};

lazy_static! {
    static ref MTCP_CONNECTIONS: Mutex<HashMap<String, TcpStream>> = Mutex::new(HashMap::new());
}

async fn handle_connection(
    mut socket: tokio::net::TcpStream,
    addr: String,
    tx: mpsc::Sender<Packet>,
) -> anyhow::Result<()> {
    let (incoming, _) = socket.split();

    info!("Incoming connection from {}", addr);

    // Delimit frames using a length header
    let length_delimited = FramedRead::new(incoming, LengthDelimitedCodec::new());

    // Deserialize frames
    let mut deserialized = tokio_serde::SymmetricallyFramed::new(
        length_delimited,
        SymmetricalJson::<Packet>::default(),
    );

    loop {
        if let Ok(res) = deserialized.try_next().await {
            if let Some(packet) = res {
                if let Err(err) = tx.try_send(packet) {
                    error!("error while passing received packet to channel ({})", err);
                }
            } else {
                break;
            }
        }
    }

    if MTCP_CONNECTIONS.lock().remove(&addr.to_string()).is_some() {
        info!("Disconnected {}", addr);
    }

    Ok(())
}

async fn listener(port: u16, tx: mpsc::Sender<Packet>) -> Result<(), io::Error> {
    let addr: SocketAddrV4 = format!("0.0.0.0:{}", port).parse().unwrap();
    let listener = TcpListener::bind(&addr)
        .await
        .expect("failed to bind tcp port");

    debug!("spawning MTCP listener on port {}", port);

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let addr = socket.peer_addr().unwrap();

        tokio::spawn(handle_connection(socket, addr.to_string(), tx.clone()));
    }
}

pub fn send_bundles(addr: String, data: Vec<u8>) -> bool {
    {
        let addr = addr.clone();
        #[allow(clippy::map_entry)]
        if !MTCP_CONNECTIONS.lock().contains_key(&addr) {
            debug!("Connecting to {}", addr);
            if let Ok(stream) = TcpStream::connect(&addr) {
                MTCP_CONNECTIONS.lock().insert(addr, stream);
            } else {
                error!("Error connecting to remote {}", addr);
                return false;
            }
        } else {
            debug!("Already connected to {}", addr);
        };
    }

    let mut s1 = MTCP_CONNECTIONS
        .lock()
        .get(&addr)
        .unwrap()
        .try_clone()
        .unwrap();

    if s1.write_all(&data).is_err() {
        error!("Error writing data to {}", addr);
        MTCP_CONNECTIONS.lock().remove(&addr);
        return false;
    }

    true
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("dtnecla json mtcp layer")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple ecla example that transmits data via tcp json encoded")
        .arg(
            Arg::new("addr")
                .short('a')
                .long("addr")
                .value_name("ip:ecla_port")
                .help("specify ecla address and port")
                .value_parser(value_parser!(String))
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .help("tcp listening port")
                .value_parser(value_parser!(u16))
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Set log level to debug")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("debug") {
        // is safe since main is single-threaded
        unsafe { std::env::set_var("RUST_LOG", "debug") };
        pretty_env_logger::init_timed();
    }

    let (tx, mut rx) = mpsc::channel::<Packet>(100);
    let (ctx, crx) = mpsc::channel::<Packet>(100);

    let port = matches.get_one::<u16>("port").expect("no port given");

    tokio::spawn(listener(*port, ctx.clone()));

    // initialize Clients
    if let Some(addr) = matches.get_one::<String>("addr") {
        info!("Connecting to {}", addr);

        let addr = addr.to_string();
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut crx = crx;
            let mut c = ws_client::new("jsonmtcp", addr.as_str(), "", tx, false)
                .expect("couldn't create client");

            let cmd_chan = c.command_channel();
            let read = tokio::spawn(async move {
                while let Some(packet) = crx.recv().await {
                    if let Err(err) = cmd_chan.send(SendPacket(packet)).await {
                        error!("couldn't pass packet to client command channel: {}", err);
                    }
                }
            });

            let connecting = c.serve();
            pin_mut!(connecting);

            future::select(connecting, read).await;
        });
    } else {
        panic!("no ecla address given");
    }

    // Read from Packet Stream
    let read = tokio::spawn(async move {
        while let Some(packet) = rx.recv().await {
            match packet {
                Packet::ForwardData(fwd) => {
                    info!("Got ForwardData {} -> {}", fwd.src, fwd.dst);

                    // Create length delimited frame [ len: u32 | frame payload ] and send to destination
                    if let Ok(mut data) = serde_json::to_vec(&Packet::ForwardData(fwd.clone())) {
                        let len = (data.len() as u32).to_be_bytes();
                        data.splice(0..0, len.iter().cloned());
                        send_bundles(fwd.dst, data);
                    }
                }
                Packet::Beacon(_) => {
                    // Beacon is not needed with MTCP
                }
                _ => {}
            }
        }
    });

    if let Err(err) = read.await {
        error!("error while joining {}", err);
    }

    Ok(())
}
