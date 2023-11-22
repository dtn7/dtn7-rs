use anyhow::{bail, Result};
use clap::{crate_authors, crate_version, value_parser, Arg, ArgAction};
use dtn7::client::data::{BundlePack, DtnPeer};
use dtn7::client::erouting::{ws_client, Packet, ResponseSenderForBundle, Sender};
use futures_util::{future, pin_mut};
use lazy_static::lazy_static;
use log::{debug, error, info};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;
use tokio::sync::mpsc;

lazy_static! {
    static ref PEERS: Mutex<BTreeMap<String, DtnPeer>> = Mutex::new(BTreeMap::new());
}

// The epidemic strategy is still fairly simple. It sends the bundles to each peer once.
// It keeps track of sent bundles in its history.
struct EpidemicStrategy {
    history: HashMap<String, HashSet<String>>,
}

impl EpidemicStrategy {
    fn new() -> EpidemicStrategy {
        EpidemicStrategy {
            history: HashMap::new(),
        }
    }

    fn add(&mut self, bundle_id: String, node_name: String) {
        let entries = self.history.entry(bundle_id).or_default();
        entries.insert(node_name);
    }

    fn contains(&self, bundle_id: &str, node_name: &str) -> bool {
        if let Some(entries) = self.history.get(bundle_id) {
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

    fn sending_failed(&mut self, bundle_id: &str, node_name: &str) {
        if let Some(entries) = self.history.get_mut(bundle_id) {
            entries.remove(node_name);
            debug!(
                "removed {:?} from sent list for bundle {}",
                node_name, bundle_id
            );
        }
    }

    fn incoming_bundle(&mut self, bundle_id: &str, node_name: &str) {
        if !node_name.is_empty() && !self.contains(bundle_id, node_name) {
            self.add(bundle_id.to_string(), node_name.to_string());
        }
    }

    fn sending_timeout(&mut self, bundle_id: &str) {
        if let Some(entries) = self.history.get_mut(bundle_id) {
            let before = entries.len();
            entries.clear();
            debug!(
                "removed {} entries from sent list for bundle {}",
                before, bundle_id
            );
        }
    }

    fn sender_for_bundle(&mut self, clas: Vec<String>, bp: BundlePack) -> (Vec<Sender>, bool) {
        let mut selected_clas: Vec<Sender> = Vec::new();
        let mut delete_afterwards = false;

        for (_, p) in PEERS.lock().unwrap().iter() {
            for c in p.cla_list.iter() {
                if clas.contains(&c.0) && !self.contains(bp.id(), &p.node_name()) {
                    self.add(bp.id().to_string(), p.node_name().clone());
                    if bp.destination.node().unwrap() == p.node_name() {
                        // direct delivery possible
                        debug!(
                            "Attempting direct delivery of bundle {} to {}",
                            bp.id(),
                            p.node_name()
                        );

                        delete_afterwards = true;
                        selected_clas.clear();
                        selected_clas.push(Sender {
                            remote: p.addr.clone(),
                            port: c.1,
                            agent: c.0.clone(),
                            next_hop: p.eid.clone(),
                        });
                        break;
                    } else {
                        debug!(
                            "Attempting delivery of bundle {} to {}",
                            bp.id(),
                            p.node_name()
                        );

                        selected_clas.push(Sender {
                            remote: p.addr.clone(),
                            port: c.1,
                            agent: c.0.clone(),
                            next_hop: p.eid.clone(),
                        });
                    }
                }
            }
        }

        (selected_clas, delete_afterwards)
    }
}

// The flooding strategy is very simple. It sends the bundle to all available peers.
fn flooding_strategy(clas: Vec<String>, _: BundlePack) -> (Vec<Sender>, bool) {
    let mut selected_clas = Vec::new();
    for (_, p) in PEERS.lock().unwrap().iter() {
        for c in p.cla_list.iter() {
            if clas.contains(&c.0) {
                selected_clas.push(Sender {
                    remote: p.addr.clone(),
                    port: c.1,
                    agent: c.0.clone(),
                    next_hop: p.eid.clone(),
                });
            }
        }
    }

    (selected_clas, false)
}

// Serve creates the connection to the external routing of dtnd and uses the given strategy.
async fn serve(strategy: String, addr: &str) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(100);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ws_client::Command>(100);

    // Create the WebSocket client.
    let client = ws_client::new(addr, tx);
    match client {
        Err(err) => {
            bail!("error while creating client: {}", err);
        }
        Ok(mut client) => {
            // Spawn the task that handles the connecting to dtnd.
            tokio::spawn(async move {
                let cmd_chan = client.command_channel();
                let read = tokio::spawn(async move {
                    while let Some(cmd) = cmd_rx.recv().await {
                        if let Err(err) = cmd_chan.send(cmd).await {
                            error!("couldn't pass packet to client command channel: {}", err);
                        }
                    }
                });

                let serving = client.serve();
                pin_mut!(serving);
                future::select(serving, read).await;
            });
        }
    }

    let mut epidemic_router = EpidemicStrategy::new();

    let read = tokio::spawn(async move {
        let strategy = strategy.clone();

        while let Some(packet) = rx.recv().await {
            match packet {
                // Overwrite own peer map with the initial state of dtnd.
                Packet::PeerState(packet) => {
                    info!("Got information about {} peers", packet.peers.len());
                    (*PEERS.lock().unwrap()) = packet.peers;
                }
                // If a new peer is encountered add it to the peer list.
                Packet::EncounteredPeer(packet) => {
                    PEERS
                        .lock()
                        .unwrap()
                        .insert(packet.eid.node().unwrap(), packet.peer);
                    info!("Peer Encountered: {}", packet.eid.node().unwrap());
                }
                // If a peer is dropped remove it from the peer list.
                Packet::DroppedPeer(packet) => {
                    PEERS
                        .lock()
                        .unwrap()
                        .remove(packet.eid.node().unwrap().as_str());
                    info!("Peer Dropped: {}", packet.eid.node().unwrap());
                }
                Packet::SendingFailed(packet) => {
                    if strategy == "epidemic" {
                        epidemic_router
                            .sending_failed(packet.bid.as_str(), packet.cla_sender.as_str());
                    }
                }
                Packet::Error(error) => {
                    error!("Error received: {}", error.reason);
                }
                Packet::Timeout(packet) => {
                    if strategy == "epidemic" {
                        epidemic_router.sending_timeout(packet.bp.id.as_str());
                    }
                }
                Packet::IncomingBundle(packet) => {
                    if strategy == "epidemic" {
                        if let Some(eid) = packet.bndl.previous_node() {
                            if let Some(node_name) = eid.node() {
                                epidemic_router.incoming_bundle(&packet.bndl.id(), &node_name);
                            }
                        };
                    }
                }
                Packet::IncomingBundleWithoutPreviousNode(packet) => {
                    if strategy == "epidemic" {
                        epidemic_router
                            .incoming_bundle(packet.bid.as_str(), packet.node_name.as_str());
                    }
                }
                Packet::RequestSenderForBundle(packet) => {
                    info!("got bundle pack: {}", packet.bp);

                    let res: (Vec<Sender>, bool) = match strategy.as_str() {
                        "flooding" => flooding_strategy(packet.clas, packet.bp.clone()),
                        "epidemic" => {
                            epidemic_router.sender_for_bundle(packet.clas, packet.bp.clone())
                        }
                        _ => (vec![], false),
                    };

                    if res.0.is_empty() {
                        info!("no cla sender could be selected");
                    } else {
                        info!("selected {} to {}", res.0[0].agent, res.0[0].remote);
                    }

                    let resp: Packet = Packet::ResponseSenderForBundle(ResponseSenderForBundle {
                        bp: packet.bp,
                        clas: res.0,
                        delete_afterwards: res.1,
                    });

                    if let Err(err) = cmd_tx
                        .send(ws_client::Command::SendPacket(Box::new(resp)))
                        .await
                    {
                        error!("send packet failed: {}", err);
                    }
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

#[tokio::main]
async fn main() -> Result<()> {
    let matches = clap::Command::new("dtn external routing example")
        .version(crate_version!())
        .author(crate_authors!())
        .about("A simple external routing example")
        .arg(
            Arg::new("addr")
                .short('a')
                .long("addr")
                .value_name("ip:erouting_port")
                .help("specify external routing address and port")
                .value_parser(value_parser!(String))
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("type")
                .short('t')
                .long("type")
                .help("specify routing type")
                .value_parser(value_parser!(String))
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .help("Set log level to debug")
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    let routing_types = ["flooding", "epidemic"];

    if matches.contains_id("debug") {
        std::env::set_var("RUST_LOG", "debug");
        pretty_env_logger::init_timed();
    }

    if !matches.contains_id("type") || !matches.contains_id("addr") {
        bail!("please specify address and strategy type");
    }

    let strategy = routing_types.iter().find(|t| {
        return matches
            .get_one::<String>("type")
            .unwrap()
            .eq_ignore_ascii_case(t);
    });

    if strategy.is_none() {
        bail!(
            "please select a strategy type from: {}",
            routing_types.join(", ")
        );
    }

    serve(
        strategy.unwrap().to_string(),
        matches.get_one::<String>("addr").unwrap(),
    )
    .await?;

    Ok(())
}
