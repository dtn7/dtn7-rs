use crate::CONFIG;
use crate::cla::ConvergenceLayerAgent;
use async_trait::async_trait;
use bp7::ByteBuffer;
use dtn7_codegen::cla;
use log::{debug, error};
use std::{
    collections::HashMap,
    io,
    net::SocketAddr,
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

// TODO: make timeout configurable
const PUSH_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn http_send_bundles(remote: String, ready: ByteBuffer) -> TransferResult {
    if ready.is_empty() {
        debug!("Nothing to forward.");
        return TransferResult::Successful;
    }

    let now = Instant::now();
    let peeraddr: SocketAddr = remote.parse().unwrap();
    let buf_len = ready.len();
    debug!("forwarding to {:?}", peeraddr);
    let req_url = format!("http://{}/push", peeraddr);

    // attohttpc is blocking, so keep it off the async runtime threads
    let result = tokio::task::spawn_blocking(move || {
        attohttpc::post(req_url)
            .header(attohttpc::header::CONTENT_TYPE, "application/octet-stream")
            .connect_timeout(PUSH_TIMEOUT)
            .timeout(PUSH_TIMEOUT)
            .bytes(ready)
            .send()
            // the remote rejects bundles it cannot decode with an error status
            .and_then(attohttpc::Response::error_for_status)
    })
    .await
    .unwrap();

    match result {
        Ok(_response) => {
            debug!(
                "Transmission time: {:?} for {} bytes to {}",
                now.elapsed(),
                buf_len,
                peeraddr
            );
            TransferResult::Successful
        }
        Err(e) => {
            if matches!(e.kind(), attohttpc::ErrorKind::Io(err)
                if matches!(err.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock))
            {
                error!(
                    "Timeout: no response in {} seconds while pushing bundle.",
                    PUSH_TIMEOUT.as_secs()
                );
            } else {
                error!("could not push bundle to remote: {}", e);
            }
            TransferResult::Failure
        }
    }
}

impl HttpConvergenceLayer {
    pub fn new(_local_settings: Option<&HashMap<String, String>>) -> HttpConvergenceLayer {
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(remote, ready, reply) => {
                        debug!(
                            "HttpConvergenceLayer: received transfer command for {}",
                            remote
                        );
                        tokio::spawn(async move {
                            reply.send(http_send_bundles(remote, ready).await).unwrap();
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
