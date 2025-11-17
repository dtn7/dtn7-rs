use crate::CONFIG;
use crate::cla::ConvergenceLayerAgent;
use async_trait::async_trait;
use bp7::ByteBuffer;
use dtn7_codegen::cla;
use log::{debug, error};
use reqwest::{Client, header};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

use super::{HelpStr, TransferResult};

#[cla(http)]
#[derive(Debug, Clone)]
pub struct HttpConvergenceLayer {
    tx: mpsc::Sender<super::ClaCmd>,
    local_port: u16,
}

pub async fn http_send_bundles(
    client: Client,
    remote: String,
    ready: ByteBuffer,
) -> TransferResult {
    if ready.is_empty() {
        debug!("Nothing to forward.");
        return TransferResult::Successful;
    }

    let now = Instant::now();

    let peeraddr: SocketAddr = remote.parse().unwrap();

    let buf_len = ready.len();
    debug!("forwarding to {:?}", peeraddr);

    // IPv6 must be bracketed in URLs
    let host = match peeraddr.ip() {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    let url = format!("http://{host}:{}/push", peeraddr.port());

    // TODO: make timeout configurable
    let req = client
        .post(url)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .timeout(Duration::from_secs(5))
        .body(ready)
        .build()
        .unwrap();

    match client.execute(req).await {
        Ok(_response) => {
            debug!(
                "Transmission time: {:?} for {} bytes to {}",
                now.elapsed(),
                buf_len,
                peeraddr
            );
            TransferResult::Successful
        }
        Err(e) if e.is_timeout() => {
            error!("Timeout: no response in 5 seconds while pushing bundle.");
            TransferResult::Failure
        }
        Err(e) => {
            error!("could not push bundle to remote: {e}");
            TransferResult::Failure
        }
    }
}

impl HttpConvergenceLayer {
    pub fn new(_local_settings: Option<&HashMap<String, String>>) -> HttpConvergenceLayer {
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let client = Client::new();
            // let client = Client::builder()
            //     .pool_idle_timeout(Duration::from_secs(15))
            //     .build()
            //     .expect("failed to build HTTP client");

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
