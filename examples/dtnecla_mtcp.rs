use anyhow::Result;
use bp7::Bundle;
use clap::{crate_authors, crate_version, App, Arg};
use dtn7::cla::ecla::ws_client::Command::SendPacket;
use dtn7::cla::ecla::{ws_client, ForwardData, Packet};
use dtn7::cla::mtcp::{MPDUCodec, MPDU};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::future::Either;
use futures_util::{future, pin_mut, StreamExt};
use lazy_static::lazy_static;
use log::{debug, error, info};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::io::Write;
use std::net::SocketAddrV4;
use std::net::TcpStream;
use std::str::FromStr;
use tokio::io;
use tokio::net::TcpListener;
use tokio_util::codec::FramedRead;

lazy_static! {
    static ref MTCP_CONNECTIONS: Mutex<HashMap<String, TcpStream>> = Mutex::new(HashMap::new());
}

async fn handle_connection(
    mut socket: tokio::net::TcpStream,
    addr: String,
    tx: UnboundedSender<Packet>,
) -> anyhow::Result<()> {
    let (incoming, _) = socket.split();

    info!("Incoming connection from {}", addr);

    let mut framed_sock = FramedRead::new(incoming, MPDUCodec::new());

    while let Some(frame) = framed_sock.next().await {
        match frame {
            Ok(frame) => {
                if let Ok(mut bndl) = Bundle::try_from(frame) {
                    info!("Received bundle: {} from {}", bndl.id(), addr);
                    {
                        if let Err(err) = tx.unbounded_send(Packet::ForwardData(ForwardData {
                            src: "".to_string(),
                            dst: addr.clone(),
                            bundle_id: bndl.id(),
                            data: bndl.to_cbor(),
                        })) {
                            info!("Error sending bundle to channel {}", err);
                        }
                    }
                } else {
                    info!("Error decoding bundle from {}", addr);
                    break;
                }
            }
            Err(err) => {
                info!("Lost connection from {} ({})", addr, err);
                break;
            }
        }
    }

    if MTCP_CONNECTIONS.lock().remove(&addr.to_string()).is_some() {
        info!("Disconnected {}", addr);
    }

    Ok(())
}

async fn listener(port: u16, tx: UnboundedSender<Packet>) -> Result<(), io::Error> {
    let port = port;
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

pub fn send_bundle(addr: String, data: Vec<u8>) -> bool {
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
    let matches = App::new("dtnecla mtcp layer")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple ecla example that transmits data via tcp cbor encoded")
        .arg(
            Arg::new("addr")
                .short('a')
                .long("addr")
                .value_name("ip:ecla_port")
                .help("specify ecla address and port")
                .multiple_values(false),
        )
        .arg(
            Arg::new("port")
                .short('p')
                .long("port")
                .help("tcp listening port")
                .multiple_values(false)
                .takes_value(true),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Set log level to debug")
                .takes_value(false),
        )
        .get_matches();

    if matches.is_present("debug") {
        std::env::set_var("RUST_LOG", "debug");
        pretty_env_logger::init_timed();
    }

    let (tx, rx) = unbounded::<Packet>();
    let (ctx, crx) = unbounded::<Packet>();

    tokio::spawn(listener(
        u16::from_str(matches.value_of("port").expect("no port given"))
            .expect("port wasn't a number"),
        ctx.clone(),
    ));

    // initialize Clients
    if let Some(addr) = matches.value_of("addr") {
        info!("Connecting to {}", addr);

        let addr = addr.to_string();
        let tx = tx.clone();
        tokio::spawn(async move {
            let crx = crx;
            let mut c = ws_client::new("mtcp", addr.as_str(), "", tx, false)
                .expect("couldn't create client");
            c.set_ecla_port(u16::from_str(matches.value_of("port").unwrap()).unwrap());

            let cmd_chan = c.command_channel();
            let read = crx.for_each(|packet| {
                cmd_chan
                    .unbounded_send(SendPacket(packet))
                    .expect("couldn't pass packet to client command channel");
                future::ready(())
            });
            let connecting = c.serve();

            pin_mut!(connecting, read);
            let res = future::select(connecting, read).await;
            if let Either::Left((con_res, _)) = res {
                if let Err(err) = con_res {
                    error!("error {}", err);
                    std::process::exit(101);
                }
            }

            std::process::exit(0);
        });
    } else {
        panic!("no ecla address given");
    }

    // Read from Packet Stream
    let read = rx.for_each(|packet| {
        match packet {
            Packet::ForwardData(fwd) => {
                info!("Got ForwardData {} -> {}", fwd.src, fwd.dst);

                if let Ok(bndl) = Bundle::try_from(fwd.data) {
                    let mpdu = MPDU::new(&bndl);
                    if let Ok(buf) = serde_cbor::to_vec(&mpdu) {
                        send_bundle(fwd.dst, buf);
                    } else {
                        error!("MPDU encoding error!");
                    }
                }
            }
            Packet::Beacon(_) => {
                // Beacon is not needed with MTCP
            }
            _ => {}
        }

        future::ready(())
    });

    pin_mut!(read);
    read.await;

    Ok(())
}
