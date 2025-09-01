use std::fmt::Display;

use crate::{CONFIG, PEERS, RoutingNotifcation};

use super::{RoutingAgent, RoutingCmd};
use async_trait::async_trait;
use glob_match::glob_match;
use log::{debug, info};
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

#[derive(Debug)]
pub struct StaticRouteEntry {
    /// index in the routing table
    pub idx: u16,
    /// source eid, wildcards are allowed
    pub src: String,
    /// destination eid, wildcards are allowed
    pub dst: String,
    /// next hop eid
    pub via: String,
}

impl Display for StaticRouteEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "#{}: route from {} to {} via {}",
            self.idx, self.src, self.dst, self.via
        )
    }
}

#[derive(Debug)]
pub struct StaticRoutingAgent {
    tx: mpsc::Sender<super::RoutingCmd>,
}

#[derive(Debug)]
pub struct StaticRoutingAgentCore {
    routes: Vec<StaticRouteEntry>,
}

impl Default for StaticRoutingAgent {
    fn default() -> Self {
        StaticRoutingAgent::new()
    }
}

impl StaticRoutingAgent {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            handle_routing_cmd(rx).await;
        });
        StaticRoutingAgent { tx }
    }
}

#[async_trait]
impl RoutingAgent for StaticRoutingAgent {
    fn channel(&self) -> Sender<crate::RoutingCmd> {
        self.tx.clone()
    }
}

impl std::fmt::Display for StaticRoutingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "StaticRoutingAgent")
    }
}

fn parse_route_from_str(s: &str) -> Option<StaticRouteEntry> {
    let s = s.trim();
    if s.starts_with('#') || s.is_empty() {
        return None;
    }
    let mut parts = s.split_whitespace();
    let idx = parts.next().unwrap().parse::<u16>().unwrap();
    let src = parts.next().unwrap();
    let dst = parts.next().unwrap();
    let via = parts.next().unwrap();
    Some(StaticRouteEntry {
        idx,
        src: src.to_string(),
        dst: dst.to_string(),
        via: via.to_string(),
    })
}

async fn handle_routing_notification(notification: RoutingNotifcation) {
    debug!("Received notification: {:?}", notification);
    match notification {
        RoutingNotifcation::SendingFailed(bid, cla_sender) => {
            debug!("Sending failed for bundle {} on CLA {}", bid, cla_sender);
        }
        RoutingNotifcation::SendingSucceeded(bid, cla_sender) => {
            debug!("Sending succeeded for bundle {} on CLA {}", bid, cla_sender);
        }
        _ => { /* ignore */ }
    }
}

async fn handle_routing_cmd(mut rx: mpsc::Receiver<RoutingCmd>) {
    let mut route_entries = vec![];
    let settings = CONFIG.lock().routing_settings.clone();

    if let Some(static_settings) = settings.get("static") {
        if let Some(routes_file) = static_settings.get("routes") {
            // open file and read routes line by line
            let routes = std::fs::read_to_string(routes_file).unwrap();
            for line in routes.lines() {
                if let Some(entry) = parse_route_from_str(line) {
                    debug!("Adding static route: {}", entry);
                    route_entries.push(entry);
                }
            }
        }
    }

    let mut core: StaticRoutingAgentCore = StaticRoutingAgentCore {
        routes: route_entries,
    };

    while let Some(cmd) = rx.recv().await {
        match cmd {
            super::RoutingCmd::SenderForBundle(bp, reply) => {
                let mut clas = vec![];
                let mut delete_afterwards = false;
                'route_loop: for route in &core.routes {
                    if glob_match(&route.src, &bp.source.to_string())
                        && glob_match(&route.dst, &bp.destination.to_string())
                    {
                        debug!(
                            "Found route: {}, looking for valid peer ({})",
                            route, route.via
                        );
                        for (_, p) in (*PEERS.lock()).iter() {
                            if p.eid.to_string() == route.via {
                                if let Some(cla) = p.first_cla() {
                                    clas.push(cla);
                                    delete_afterwards = !bp.destination.is_non_singleton();
                                    break 'route_loop;
                                }
                            }
                        }
                        debug!("No valid peer found for route {}", route)
                    }
                }
                if clas.is_empty() {
                    debug!("No route found for bundle {}", bp);
                }
                reply.send((clas, delete_afterwards)).unwrap();
            }
            super::RoutingCmd::Shutdown => {
                break;
            }
            super::RoutingCmd::Command(cmd) => {
                if cmd == "reload" {
                    let settings = CONFIG.lock().routing_settings.clone();
                    if let Some(static_settings) = settings.get("static") {
                        if let Some(routes_file) = static_settings.get("routes") {
                            info!("Reloading static routes from {}", routes_file);
                            // open file and read routes line by line
                            let routes = std::fs::read_to_string(routes_file).unwrap();
                            let mut route_entries = vec![];
                            for line in routes.lines() {
                                if let Some(entry) = parse_route_from_str(line) {
                                    debug!("Adding static route: {}", entry);
                                    route_entries.push(entry);
                                }
                            }
                            core.routes = route_entries;
                        }
                    }
                } else {
                    debug!("Unknown command: {}", cmd);
                }
            }
            super::RoutingCmd::GetData(_, tx) => {
                let routes_as_str = core
                    .routes
                    .iter()
                    .fold(String::new(), |acc, r| acc + &format!("{}\n", r));
                tx.send(routes_as_str).unwrap();
            }
            super::RoutingCmd::Notify(notification) => {
                handle_routing_notification(notification).await;
            }
        }
    }
}
