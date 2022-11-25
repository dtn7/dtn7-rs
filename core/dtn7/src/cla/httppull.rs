use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::IpAddr;

use crate::core::helpers::get_complete_digest;
use crate::core::peer::PeerAddress;
use crate::{store_has_item, CONFIG};

use super::TransferResult;
use super::{ConvergenceLayerAgent, HelpStr};
use async_trait::async_trait;
use bp7::EndpointID;
use dtn7_codegen::cla;
use log::{debug, error, info, warn};
use tokio::select;
use tokio::sync::mpsc;

#[cla(httppull)]
#[derive(Debug, Clone)]
pub struct HttpPullConvergenceLayer {
    tx: mpsc::Sender<super::ClaCmd>,
}

async fn http_pull_from_node(
    eid: EndpointID,
    addr: IpAddr,
    port: u16,
    local_digest: String,
) -> TransferResult {
    debug!("pulling bundles from {} / {}", eid, addr);

    // get digest of remote node
    let response = reqwest::get(&format!("http://{}:{}/status/bundles/digest", addr, port)).await;
    let digest = match response {
        Ok(digest) => digest.text().await.unwrap(),
        Err(e) => {
            error!("could not get digest from remote: {}", e);
            return TransferResult::Failure;
        }
    };
    if digest == local_digest {
        debug!("no new bundles on remote");
        return TransferResult::Successful;
    }
    // get list of bundles from remote node
    let response = reqwest::get(&format!("http://{}:{}/status/bundles", addr, port)).await;
    let bid_list = match response {
        Ok(bid_list) => bid_list.text().await.unwrap(),
        Err(e) => {
            error!("could not get bundle ID list from remote: {}", e);
            return TransferResult::Failure;
        }
    };
    let bids: Vec<String> = serde_json::from_str(&bid_list).unwrap();

    // calculate missing bundles
    let mut missing = Vec::new();

    for bid in bids {
        if !store_has_item(&bid) {
            missing.push(bid);
        }
    }

    // fetch missing bundles from remote node
    for bid in missing {
        let response = reqwest::get(&format!("http://{}:{}/download?{}", addr, port, bid)).await;
        let bundle_buf = match response {
            Ok(bundle) => bundle.bytes().await.unwrap(),
            Err(e) => {
                error!("could not get bundle from remote: {}", e);
                return TransferResult::Failure;
            }
        };
        let bundle = match bp7::Bundle::try_from(bundle_buf.as_ref()) {
            Ok(bundle) => bundle,
            Err(e) => {
                warn!("could not parse bundle from remote: {}", e);
                continue;
            }
        };
        info!("Downloaded bundle: {} from {}", bundle.id(), addr);
        {
            tokio::spawn(async move {
                if let Err(err) = crate::core::processing::receive(bundle).await {
                    error!("Failed to process bundle: {}", err);
                }
            });
        }
    }
    TransferResult::Successful
}
async fn http_pull_bundles() {
    debug!("pulling bundles from peers");

    let local_digest = get_complete_digest();

    let peers = crate::PEERS.lock().clone();
    for (_, p) in peers.iter() {
        if let PeerAddress::Ip(ipaddr) = p.addr {
            let peer = p.clone();
            let local_digest = local_digest.clone();
            let mut port = 3000;
            for cla in p.cla_list.iter() {
                if cla.0 == "httppull" {
                    if let Some(p) = cla.1 {
                        port = p;
                        break;
                    }
                }
            }
            if CONFIG.lock().parallel_bundle_processing {
                tokio::spawn(async move {
                    http_pull_from_node(peer.eid, ipaddr, port, local_digest).await;
                });
            } else {
                http_pull_from_node(peer.eid, ipaddr, port, local_digest).await;
            }
        }
    }
    debug!("finished pulling bundles from peers");
}
async fn http_puller_loop(rx: mpsc::Receiver<bool>) {
    let mut rx = rx;
    let interval = CONFIG.lock().janitor_interval;
    loop {
        select! {
          _ = rx.recv() => {
            debug!("received shutdown command");
            break;
          }
          _ = tokio::time::sleep(interval) => {
            http_pull_bundles().await;
          }
        }
    }
}

impl HttpPullConvergenceLayer {
    pub fn new(_local_settings: Option<&HashMap<String, String>>) -> HttpPullConvergenceLayer {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        tokio::spawn(async move {
            http_puller_loop(shutdown_rx).await;
        });
        let (tx, mut rx) = mpsc::channel(1);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(remote, _, reply) => {
                        debug!(
                            "HttpPullConvergenceLayer: received transfer command for {}",
                            remote
                        );
                        reply.send(TransferResult::Failure).unwrap();
                    }
                    super::ClaCmd::Shutdown => {
                        debug!("HttpPullConvergenceLayer: received shutdown command");
                        shutdown_tx.send(true).await.unwrap();
                        break;
                    }
                }
            }
        });
        HttpPullConvergenceLayer { tx }
    }
}

#[async_trait]
impl ConvergenceLayerAgent for HttpPullConvergenceLayer {
    async fn setup(&mut self) {}

    fn port(&self) -> u16 {
        0
    }

    fn name(&self) -> &str {
        // my_name() is generated from cla proc macro attribute
        self.my_name()
    }

    fn channel(&self) -> tokio::sync::mpsc::Sender<super::ClaCmd> {
        self.tx.clone()
    }
    fn accepting(&self) -> bool {
        false
    }
}

impl HelpStr for HttpPullConvergenceLayer {}

impl std::fmt::Display for HttpPullConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "httppull")
    }
}
