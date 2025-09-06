use anyhow::Result;
use clap::{crate_authors, crate_version, Arg, ArgAction, Command};
use dtn7::client::ecla::ws_client::Command::SendPacket;
use dtn7::client::ecla::Packet::{Beacon, ForwardData};
use dtn7::client::ecla::{ws_client, Packet};
use futures_util::{future, pin_mut};
use log::{error, info};
use std::str::FromStr;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("dtnecla connect N")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple ecla example that connects N dtnd instances")
        .arg(
            Arg::new("addr")
                .short('a')
                .long("addr")
                .value_name("ip:ecla_port")
                .help("specify ecla address and port")
                .action(ArgAction::Append),
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

    // initialize Clients
    let mut conns: Vec<mpsc::Sender<Packet>> = vec![];
    if let Some(addrs) = matches.get_many::<String>("addr") {
        for (i, addr) in addrs.enumerate() {
            info!("Connecting to {}", addr);

            let (ctx, crx) = mpsc::channel::<Packet>(100);
            conns.push(ctx);

            let addr = addr.to_string();
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut crx = crx;
                let mut c =
                    ws_client::new("ConnectN", addr.as_str(), i.to_string().as_str(), tx, true)
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
        }
    }

    // Read from Packet Stream
    let read = tokio::spawn(async move {
        while let Some(packet) = rx.recv().await {
            match packet {
                Packet::ForwardData(fwd) => {
                    let dst: Vec<&str> = fwd.dst.split(':').collect();
                    info!("Got ForwardData {} -> {}", fwd.src, dst[0]);

                    let id = usize::from_str(dst[0]).unwrap_or(conns.len());
                    if id < conns.len() {
                        if let Err(err) = conns[id].send(ForwardData(fwd.clone())).await {
                            error!("couldn't pass packet to client packet channel: {}", err)
                        }
                    }
                }
                Packet::Beacon(pdp) => {
                    info!("Got Beacon {}", pdp.addr);

                    let id = usize::from_str(pdp.addr.as_str()).unwrap_or(conns.len());
                    conns.iter().enumerate().for_each(|(i, conn)| {
                        if i == id {
                            return;
                        }

                        if let Err(err) = conn.try_send(Beacon(pdp.clone())) {
                            error!("couldn't pass packet to client packet channel: {}", err)
                        }
                    });
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
