use anyhow::{bail, Result};
use clap::{crate_authors, crate_version, App, Arg};
use dtn7::dtnd::ecla::ws_client::{new, Client};
use dtn7::dtnd::ecla::Packet;
use dtn7::dtnd::ecla::Packet::{Beacon, ForwardDataPacket};
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
                .short("A")
                .long("addr")
                .value_name("ip:ecla_port")
                .help("specify ecla address and port")
                .multiple(true)
                .takes_value(true),
        )
        .get_matches();

    std::env::set_var(
        "RUST_LOG",
        "dtn7=debug,dtnd=debug,actix_server=debug,actix_web=debug,debug,dtnecla_connect_n=debug",
    );
    pretty_env_logger::init_timed();

    let (tx, rx) = unbounded::<Packet>();

    // initialize Clients
    let mut conns: Vec<UnboundedSender<Packet>> = vec![];
    if let Some(addrs) = matches.values_of("addr") {
        for (i, addr) in addrs.enumerate() {
            println!("{}", addr);

            let (ctx, crx) = unbounded::<Packet>();
            conns.push(ctx);

            let addr = addr.to_string();
            let tx = tx.clone();
            tokio::spawn(async move {
                let crx = crx;
                let mut c = new("ConnectN", addr.as_str(), i.to_string().as_str(), tx)
                    .expect("couldn't create client");

                let connecting = c.connect();
                let read = crx.for_each(|packet| {
                    match packet {
                        Packet::ForwardDataPacket(mut fwd) => {
                            c.insert_forward_data(fwd);
                        }
                        Packet::Beacon(mut pdp) => {
                            c.insert_beacon(pdp);
                        }
                        _ => {}
                    }

                    future::ready(())
                });

                pin_mut!(connecting, read);
                future::select(connecting, read).await;
            });
        }
    }

    // Read from Packet Stream
    let read = rx.for_each(|packet| {
        match packet {
            Packet::ForwardDataPacket(mut fwd) => {
                info!("Got ForwardDataPacket {} -> {}", fwd.src, fwd.dst);

                let id = usize::from_str(fwd.dst.as_str()).unwrap_or(conns.len());
                if id < conns.len() {
                    conns[id].unbounded_send(ForwardDataPacket(fwd.clone()));
                }
            }
            Packet::Beacon(mut pdp) => {
                info!("Got Beacon {}", pdp.addr);

                let id = usize::from_str(pdp.addr.as_str()).unwrap_or(conns.len());
                if id < conns.len() {
                    conns[id].unbounded_send(Beacon(pdp.clone()));
                }
            }
            _ => {}
        }

        future::ready(())
    });

    pin_mut!(read);
    read.await;

    Ok(())
}
