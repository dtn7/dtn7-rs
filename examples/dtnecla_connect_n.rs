use anyhow::Result;
use clap::{crate_authors, crate_version, App, Arg};
use dtn7::cla::ecla::ws_client::new;
use dtn7::cla::ecla::ws_client::Command::SendPacket;
use dtn7::cla::ecla::Packet;
use dtn7::cla::ecla::Packet::{Beacon, ForwardDataPacket};
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures_util::{future, pin_mut, StreamExt};
use log::info;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("dtnecla connect N")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple ecla example that connects N dtnd instances")
        .arg(
            Arg::with_name("addr")
                .short("a")
                .long("addr")
                .value_name("ip:ecla_port")
                .help("specify ecla address and port")
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("debug")
                .short("d")
                .long("debug")
                .help("Set log level to debug")
                .takes_value(false),
        )
        .get_matches();

    if matches.is_present("debug") {
        std::env::set_var(
            "RUST_LOG",
            "dtn7=debug,dtnd=debug,actix_server=debug,actix_web=debug,debug,dtnecla_connect_n=debug",
        );
        pretty_env_logger::init_timed();
    }

    let (tx, rx) = unbounded::<Packet>();

    // initialize Clients
    let mut conns: Vec<UnboundedSender<Packet>> = vec![];
    if let Some(addrs) = matches.values_of("addr") {
        for (i, addr) in addrs.enumerate() {
            info!("Connecting to {}", addr);

            let (ctx, crx) = unbounded::<Packet>();
            conns.push(ctx);

            let i = i;
            let addr = addr.to_string();
            let tx = tx.clone();
            tokio::spawn(async move {
                let crx = crx;
                let mut c = new("ConnectN", addr.as_str(), i.to_string().as_str(), tx, true)
                    .expect("couldn't create client");

                let cmd_chan = c.command_channel();
                let read = crx.for_each(|packet| {
                    cmd_chan
                        .unbounded_send(SendPacket(packet))
                        .expect("couldn't pass packet to client command channel");
                    future::ready(())
                });
                let connecting = c.connect();

                pin_mut!(connecting, read);
                future::select(connecting, read).await;
            });
        }
    }

    // Read from Packet Stream
    let read = rx.for_each(|packet| {
        match packet {
            Packet::ForwardDataPacket(fwd) => {
                info!("Got ForwardDataPacket {} -> {}", fwd.src, fwd.dst);

                let id = usize::from_str(fwd.dst.as_str()).unwrap_or(conns.len());
                if id < conns.len() {
                    conns[id]
                        .unbounded_send(ForwardDataPacket(fwd.clone()))
                        .expect("couldn't pass packet to client packet channel");
                }
            }
            Packet::Beacon(pdp) => {
                info!("Got Beacon {}", pdp.addr);

                let id = usize::from_str(pdp.addr.as_str()).unwrap_or(conns.len());
                conns.iter().enumerate().for_each(|(i, conn)| {
                    if i == id {
                        return;
                    }
                    conn.unbounded_send(Beacon(pdp.clone()))
                        .expect("couldn't pass packet to client packet channel");
                });
            }
            _ => {}
        }

        future::ready(())
    });

    pin_mut!(read);
    read.await;

    Ok(())
}
