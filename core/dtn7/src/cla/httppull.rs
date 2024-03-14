use std::collections::HashMap;
use std::convert::TryFrom;

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

/// pulls missing bundles from node
/// addr can be either an IP or a DNS name
async fn http_pull_from_node(
    eid: EndpointID,
    addr: String,
    port: u16,
    local_digest: String,
) -> TransferResult {
    let now = std::time::Instant::now();
    let mut transfers = 0;

    debug!("pulling bundles from {} / {}", eid, addr);
    tokio::task::spawn_blocking(move || {
        // get digest of remote node
        let response =
            attohttpc::get(format!("http://{}:{}/status/bundles/digest", addr, port)).send();
        let digest = match response {
            Ok(digest) => digest.text().unwrap(),
            Err(e) => {
                error!("could not get digest from remote: {}", e);
                //bail!("could not get digest from remote: {}", e);
                return TransferResult::Failure;
            }
        };
        if digest == local_digest {
            debug!("no new bundles on remote");
            return TransferResult::Successful;
        } else {
            debug!(
                "remote ({}) has new bundles (remote: {} vs local: {})",
                eid, digest, local_digest
            );
        }
        // get list of bundles from remote node
        let response = attohttpc::get(format!("http://{}:{}/status/bundles", addr, port)).send();
        let bid_list = match response {
            Ok(bid_list) => bid_list.text().unwrap(),
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
            transfers += 1;
            let response =
                attohttpc::get(format!("http://{}:{}/download?{}", addr, port, bid)).send();

            let bundle_buf = match response {
                Ok(bundle) => bundle.bytes().unwrap(),
                Err(e) => {
                    error!("could not get bundle from remote: {}", e);
                    return TransferResult::Failure;
                }
            };
            let bundle = match bp7::Bundle::try_from(bundle_buf.as_ref()) {
                Ok(bundle) => bundle,
                Err(e) => {
                    crate::STATS.lock().broken += 1;
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
        debug!(
            "finished pulling {} bundles from {} / {} in {:?}",
            transfers,
            eid,
            addr,
            now.elapsed()
        );
        TransferResult::Successful
    })
    .await
    .unwrap()
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
                    http_pull_from_node(peer.eid, ipaddr.to_string(), port, local_digest).await;
                });
            } else {
                http_pull_from_node(peer.eid, ipaddr.to_string(), port, local_digest).await;
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
            let now = std::time::Instant::now();
            http_pull_bundles().await;
            debug!("http puller took {:?}", now.elapsed());
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
