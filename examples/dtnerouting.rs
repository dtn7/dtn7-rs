use anyhow::{bail, Result};
use clap::{crate_authors, crate_version, App, Arg};
use dtn7::cla::ClaSender;
use dtn7::dtnd::erouting::ws_client::{new, Command};
use dtn7::dtnd::erouting::{Packet, SendForBundleResponsePacket};
use dtn7::DtnPeer;
use futures::channel::mpsc::unbounded;
use futures_util::{future, pin_mut, StreamExt};
use log::info;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("dtn external routing example")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple external routing example")
        .arg(
            Arg::with_name("addr")
                .short("a")
                .long("addr")
                .value_name("ip:erouting_port")
                .help("specify external routing address and port")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("type")
                .short("t")
                .long("type")
                .help("specify routing type")
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

    let routing_types = vec!["flooding"];

    if matches.is_present("debug") {
        std::env::set_var(
            "RUST_LOG",
            "dtn7=debug,dtnd=debug,actix_server=debug,actix_web=debug,dtnerouting=debug,dtnerouting=info,debug,info",
        );
        pretty_env_logger::init_timed();
    }

    if !matches.is_present("type") || !matches.is_present("addr") {
        bail!("please specify address and type");
    }

    let selected_type = routing_types.iter().find(|t| {
        return matches.value_of("type").unwrap().eq_ignore_ascii_case(t);
    });

    if selected_type.is_none() {
        bail!("please select a type from: {}", routing_types.join(", "));
    }

    let (tx, rx) = unbounded::<Packet>();
    let (cmd_tx, cmd_rx) = unbounded::<Command>();

    let client = new(matches.value_of("addr").unwrap(), tx);

    match client {
        Err(err) => {
            bail!("error while creating client: {}", err);
        }
        Ok(mut client) => {
            tokio::spawn(async move {
                let cmd_chan = client.command_channel();
                let read = cmd_rx.for_each(|cmd| {
                    cmd_chan
                        .unbounded_send(cmd)
                        .expect("couldn't pass packet to client command channel");
                    future::ready(())
                });
                let connecting = client.connect();

                pin_mut!(connecting, read);
                future::select(connecting, read).await;
            });
        }
    }

    let mut peers: HashMap<String, DtnPeer> = HashMap::new();

    let read = rx.for_each(|packet| {
        match packet {
            Packet::PeerStatePacket(pps) => {
                peers = pps.peers;
                info!("Peer State: {}", peers.len());
            }
            Packet::EncounteredPeerPacket(epp) => {
                peers.insert(epp.eid.node().unwrap(), epp.peer);
                info!("Peer Encountered: {}", epp.eid.node().unwrap());
            }
            Packet::DroppedPeerPacket(dpp) => {
                peers.remove(dpp.eid.node().unwrap().as_str());
                info!("Peer Dropped: {}", dpp.eid.node().unwrap());
            }
            Packet::SendForBundlePacket(sfbp) => match *selected_type.unwrap() {
                "flooding" => {
                    let mut clas = Vec::new();
                    for (_, p) in peers.iter() {
                        for c in p.cla_list.iter() {
                            if sfbp.clas.contains(&c.0) {
                                clas.push(ClaSender {
                                    remote: p.addr.clone(),
                                    agent: c.0.clone(),
                                })
                            }
                        }
                    }

                    let resp: Packet =
                        Packet::SendForBundleResponsePacket(SendForBundleResponsePacket { clas });

                    cmd_tx
                        .unbounded_send(Command::SendPacket(resp))
                        .expect("send packet failed");
                }
                "epidemic" => {}
                _ => {}
            },
            _ => {}
        }

        future::ready(())
    });

    pin_mut!(read);
    read.await;

    Ok(())
}
