use crate::cla::ConvergenceLayerAgent;
use crate::CONFIG;
use async_trait::async_trait;
use bp7::ByteBuffer;
use dtn7_codegen::cla;
use hyper::{client::HttpConnector, Body, Client, Method, Request};
use log::{debug, error};
use std::{collections::HashMap, net::SocketAddr, time::Instant};
use tokio::sync::mpsc;

use super::{HelpStr, TransferResult};

#[cla(http)]
#[derive(Debug, Clone)]
pub struct HttpConvergenceLayer {
    tx: mpsc::Sender<super::ClaCmd>,
    local_port: u16,
}

pub async fn http_send_bundles(
    client: Client<HttpConnector>,
    remote: String,
    ready: ByteBuffer,
) -> TransferResult {
    if !ready.is_empty() {
        let now = Instant::now();
        //let client = hyper::client::Client::new();
        let peeraddr: SocketAddr = remote.parse().unwrap();
        let buf_len = ready.len();
        debug!("forwarding to {:?}", peeraddr);
        //for b in &ready {
        let req_url = format!("http://{}:{}/push", peeraddr.ip(), peeraddr.port());
        let req = Request::builder()
            .method(Method::POST)
            .uri(req_url)
            .header("content-type", "application/octet-stream")
            .body(Body::from(ready))
            .unwrap();
        // TODO: make timout configurable
        match tokio::time::timeout(std::time::Duration::from_secs(5), client.request(req)).await {
            Ok(result) => match result {
                Ok(_response) => debug!(
                    "Transmission time: {:?} for {} bytes to {}",
                    now.elapsed(),
                    buf_len,
                    peeraddr
                ),
                Err(e) => {
                    error!("could not push bundle to remote: {}", e);
                    return TransferResult::Failure;
                }
            },
            Err(_) => {
                error!("Timeout: no response in 5 seconds while pushing bundle.");
                return TransferResult::Failure;
            }
        }
        //}
        //debug!("successfully sent {} bundles to {}", ready.len(), remote);
    } else {
        debug!("Nothing to forward.");
    }
    TransferResult::Successful
}

impl HttpConvergenceLayer {
    pub fn new(_local_settings: Option<&HashMap<String, String>>) -> HttpConvergenceLayer {
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let client = hyper::client::Client::new();
            /*let client = hyper::client::Client::builder()
            .pool_idle_timeout(Duration::from_secs(15))
            .retry_canceled_requests(false)
            .set_host(false)
            .build_http();*/

            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(remote, ready, reply) => {
                        debug!(
                            "HttpConvergenceLayer: received transfer command for {}",
                            remote
                        );
                        let client2 = client.clone();
                        tokio::spawn(async move {
                            reply
                                .send(http_send_bundles(client2, remote, ready).await)
                                .unwrap();
                        });
                    }
                    super::ClaCmd::Shutdown => {
                        debug!("HttpConvergenceLayer: received shutdown command");
                        break;
                    }
                }
            }
        });
        HttpConvergenceLayer {
            local_port: CONFIG.lock().webport,
            tx,
        }
    }
}

#[async_trait]
impl ConvergenceLayerAgent for HttpConvergenceLayer {
    async fn setup(&mut self) {}
    fn port(&self) -> u16 {
        self.local_port
    }
    fn name(&self) -> &str {
        "http"
    }
    fn channel(&self) -> tokio::sync::mpsc::Sender<super::ClaCmd> {
        self.tx.clone()
    }
}

impl HelpStr for HttpConvergenceLayer {}

impl std::fmt::Display for HttpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "http:{}", self.local_port)
    }
}
