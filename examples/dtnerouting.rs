use anyhow::{bail, Result};
use clap::{crate_authors, crate_version, App, Arg};
use dtn7::cla::ClaSender;
use dtn7::dtnd::erouting::ws_client::{new, Command};
use dtn7::dtnd::erouting::{Packet, SendForBundleResponsePacket};
use dtn7::DtnPeer;
use futures::channel::mpsc::unbounded;
use futures_util::{future, pin_mut, StreamExt};
use lazy_static::lazy_static;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

lazy_static! {
    pub static ref HISTORY: Mutex<HashMap<String, HashSet<String>>> = Mutex::new(HashMap::new());
}

fn epi_add(bundle_id: String, node_name: String) {
    HISTORY
        .lock()
        .unwrap()
        .entry(bundle_id)
        .or_insert_with(HashSet::new)
        .insert(node_name);
}

fn epi_contains(bundle_id: &str, node_name: &str) -> bool {
    if let Some(entries) = HISTORY.lock().unwrap().get(bundle_id) {
        return entries.contains(node_name);
    }
    false
}

fn epi_sending_failed(bundle_id: &str, node_name: &str) {
    if let Some(entries) = HISTORY.lock().unwrap().get_mut(bundle_id) {
        entries.remove(node_name);
        debug!(
            "removed {:?} from sent list for bundle {}",
            node_name, bundle_id
        );
    }
}

fn epi_incoming_bundle(bundle_id: &str, node_name: &str) {
    if !node_name.is_empty() && !epi_contains(bundle_id, node_name) {
        epi_add(bundle_id.to_string(), node_name.to_string());
    }
}

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

    let routing_types = vec!["flooding", "epidemic"];

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

    info!("selected routing: {}", selected_type.unwrap());

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
            Packet::PeerStatePacket(packet) => {
                peers = packet.peers;
                info!("Peer State: {}", peers.len());
            }
            Packet::EncounteredPeerPacket(packet) => {
                peers.insert(packet.eid.node().unwrap(), packet.peer);
                info!("Peer Encountered: {}", packet.eid.node().unwrap());
            }
            Packet::DroppedPeerPacket(packet) => {
                peers.remove(packet.eid.node().unwrap().as_str());
                info!("Peer Dropped: {}", packet.eid.node().unwrap());
            }
            Packet::SendingFailedPacket(packet) => {
                if *selected_type.unwrap() == "epidemic" {
                    epi_sending_failed(packet.bid.as_str(), packet.cla_sender.as_str());
                }
            }
            Packet::IncomingBundlePacket(packet) => {
                if *selected_type.unwrap() == "epidemic" {
                    if let Some(eid) = packet.bndl.previous_node() {
                        if let Some(node_name) = eid.node() {
                            epi_incoming_bundle(&packet.bndl.id(), &node_name);
                        }
                    };
                }
            }
            Packet::IncomingBundleWithoutPreviousNodePacket(packet) => {
                if *selected_type.unwrap() == "epidemic" {
                    epi_incoming_bundle(packet.bid.as_str(), packet.node_name.as_str());
                }
            }
            Packet::SendForBundlePacket(packet) => match *selected_type.unwrap() {
                "flooding" => {
                    let mut clas = Vec::new();
                    for (_, p) in peers.iter() {
                        for c in p.cla_list.iter() {
                            if packet.clas.contains(&c.0) {
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
                "epidemic" => {
                    let mut clas = Vec::new();
                    for (_, p) in peers.iter() {
                        for c in p.cla_list.iter() {
                            if packet.clas.contains(&c.0)
                                && !epi_contains(packet.bp.id(), &p.node_name())
                            {
                                clas.push(ClaSender {
                                    remote: p.addr.clone(),
                                    agent: c.0.clone(),
                                });
                                epi_add(packet.bp.id().to_string(), p.node_name().clone());
                            }
                        }
                    }

                    if clas.is_empty() {
                        info!("no cla sender could be selected");
                    } else {
                        info!("selected {} to {}", clas[0].agent, clas[0].remote);
                    }

                    let resp: Packet =
                        Packet::SendForBundleResponsePacket(SendForBundleResponsePacket { clas });

                    cmd_tx
                        .unbounded_send(Command::SendPacket(resp))
                        .expect("send packet failed");
                }
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
