use anyhow::{bail, Result};
use clap::{crate_authors, crate_version, App, Arg};
use dtn7::routing::erouting::ws_client::{new, Command};
use dtn7::routing::erouting::{Packet, Sender, SenderForBundleResponse};
use dtn7::DtnPeer;
use futures::channel::mpsc::unbounded;
use futures_util::{future, pin_mut, StreamExt};
use log::{debug, info};
use std::collections::{HashMap, HashSet};

fn epi_add(history: &mut HashMap<String, HashSet<String>>, bundle_id: String, node_name: String) {
    let entries = history.entry(bundle_id).or_insert_with(HashSet::new);
    entries.insert(node_name);
}

fn epi_contains(
    history: &mut HashMap<String, HashSet<String>>,
    bundle_id: &str,
    node_name: &str,
) -> bool {
    if let Some(entries) = history.get(bundle_id) {
        debug!(
            "Contains: {} {} {}",
            bundle_id,
            node_name,
            entries.contains(node_name)
        );
        return entries.contains(node_name);
    }
    false
}

fn epi_sending_failed(
    history: &mut HashMap<String, HashSet<String>>,
    bundle_id: &str,
    node_name: &str,
) {
    if let Some(entries) = history.get_mut(bundle_id) {
        entries.remove(node_name);
        debug!(
            "removed {:?} from sent list for bundle {}",
            node_name, bundle_id
        );
    }
}

fn epi_incoming_bundle(
    history: &mut HashMap<String, HashSet<String>>,
    bundle_id: &str,
    node_name: &str,
) {
    if !node_name.is_empty() && !epi_contains(history, bundle_id, node_name) {
        epi_add(history, bundle_id.to_string(), node_name.to_string());
    }
}

fn epi_sending_timeout(history: &mut HashMap<String, HashSet<String>>, bundle_id: &str) {
    if let Some(entries) = history.get_mut(bundle_id) {
        let before = entries.len();
        entries.clear();
        debug!(
            "removed {} entries from sent list for bundle {}",
            before, bundle_id
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new("dtn external routing example")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple external routing example")
        .arg(
            Arg::new("addr")
                .short('a')
                .long("addr")
                .value_name("ip:erouting_port")
                .help("specify external routing address and port")
                .takes_value(true),
        )
        .arg(
            Arg::new("type")
                .short('t')
                .long("type")
                .help("specify routing type")
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

    let routing_types = vec!["flooding", "epidemic"];

    if matches.is_present("debug") {
        std::env::set_var("RUST_LOG", "debug");
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

    let selected_type: &str = selected_type.unwrap();

    info!("selected routing: {}", selected_type);

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
                let connecting = client.serve();

                pin_mut!(connecting, read);
                future::select(connecting, read).await;
            });
        }
    }

    let mut history: HashMap<String, HashSet<String>> = HashMap::new();
    let mut peers: HashMap<String, DtnPeer> = HashMap::new();

    let read = rx.for_each(|packet| {
        match packet {
            Packet::PeerState(packet) => {
                peers = packet.peers;
                info!("Peer State: {}", peers.len());
            }
            Packet::EncounteredPeer(packet) => {
                peers.insert(packet.eid.node().unwrap(), packet.peer);
                info!("Peer Encountered: {}", packet.eid.node().unwrap());
            }
            Packet::DroppedPeer(packet) => {
                peers.remove(packet.eid.node().unwrap().as_str());
                info!("Peer Dropped: {}", packet.eid.node().unwrap());
            }
            Packet::SendingFailed(packet) => {
                if selected_type == "epidemic" {
                    debug!("Node: {}", packet.cla_sender.as_str());
                    epi_sending_failed(
                        &mut history,
                        packet.bid.as_str(),
                        packet.cla_sender.as_str(),
                    );
                }
            }
            Packet::Timeout(packet) => {
                if selected_type == "epidemic" {
                    epi_sending_timeout(&mut history, packet.bp.id.as_str());
                }
            }
            Packet::IncomingBundle(packet) => {
                if selected_type == "epidemic" {
                    if let Some(eid) = packet.bndl.previous_node() {
                        if let Some(node_name) = eid.node() {
                            epi_incoming_bundle(&mut history, &packet.bndl.id(), &node_name);
                        }
                    };
                }
            }
            Packet::IncomingBundleWithoutPreviousNode(packet) => {
                if selected_type == "epidemic" {
                    epi_incoming_bundle(
                        &mut history,
                        packet.bid.as_str(),
                        packet.node_name.as_str(),
                    );
                }
            }
            Packet::SenderForBundle(packet) => {
                info!("got bundle pack: {}", packet.bp);

                let mut clas = Vec::new();
                let mut delete_afterwards = false;

                match selected_type {
                    "flooding" => {
                        for (_, p) in peers.iter() {
                            for c in p.cla_list.iter() {
                                if packet.clas.contains(&c.0) {
                                    clas.push(Sender {
                                        remote: p.addr.clone(),
                                        port: c.1,
                                        agent: c.0.clone(),
                                        next_hop: p.eid.clone(),
                                    });
                                }
                            }
                        }
                    }
                    "epidemic" => {
                        for (_, p) in peers.iter() {
                            for c in p.cla_list.iter() {
                                if packet.clas.contains(&c.0)
                                    && !epi_contains(&mut history, packet.bp.id(), &p.node_name())
                                {
                                    epi_add(
                                        &mut history,
                                        packet.bp.id().to_string(),
                                        p.node_name().clone(),
                                    );
                                    if packet.bp.destination.node().unwrap() == p.node_name() {
                                        // direct delivery possible
                                        debug!(
                                            "Attempting direct delivery of bundle {} to {}",
                                            packet.bp.id(),
                                            p.node_name()
                                        );

                                        delete_afterwards = true;
                                        clas.clear();
                                        clas.push(Sender {
                                            remote: p.addr.clone(),
                                            port: c.1,
                                            agent: c.0.clone(),
                                            next_hop: p.eid.clone(),
                                        });
                                        break;
                                    } else {
                                        debug!(
                                            "Attempting delivery of bundle {} to {}",
                                            packet.bp.id(),
                                            p.node_name()
                                        );

                                        clas.push(Sender {
                                            remote: p.addr.clone(),
                                            port: c.1,
                                            agent: c.0.clone(),
                                            next_hop: p.eid.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                if clas.is_empty() {
                    info!("no cla sender could be selected");
                } else {
                    info!("selected {} to {}", clas[0].agent, clas[0].remote);
                }

                let resp: Packet = Packet::SenderForBundleResponse(SenderForBundleResponse {
                    bp: packet.bp,
                    clas,
                    delete_afterwards,
                });

                cmd_tx
                    .unbounded_send(Command::SendPacket(Box::new(resp)))
                    .expect("send packet failed");
            }
            _ => {}
        }

        future::ready(())
    });

    pin_mut!(read);
    read.await;

    Ok(())
}
