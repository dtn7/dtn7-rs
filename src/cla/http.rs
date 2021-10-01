use crate::cla::ConvergenceLayerAgent;
use crate::CONFIG;
use async_trait::async_trait;
use bp7::ByteBuffer;
use hyper::{Body, Method, Request};
use log::{debug, error};
use std::net::SocketAddr;

#[derive(Debug, Clone, Default, Copy)]
pub struct HttpConvergenceLayer {
    counter: u64,
    local_port: u16,
}

impl HttpConvergenceLayer {
    pub fn new(port: Option<u16>) -> HttpConvergenceLayer {
        HttpConvergenceLayer {
            counter: 0,
            local_port: port.unwrap_or((*CONFIG.lock()).webport),
        }
    }
}

#[async_trait]
impl ConvergenceLayerAgent for HttpConvergenceLayer {
    async fn setup(&mut self) {}
    fn port(&self) -> u16 {
        self.local_port
    }
    fn name(&self) -> &'static str {
        "http"
    }
    async fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
        debug!("Scheduled HTTP submission: {:?}", dest);
        if !ready.is_empty() {
            let client = hyper::client::Client::new();
            let peeraddr: SocketAddr = dest.parse().unwrap();
            debug!("forwarding to {:?}", peeraddr);
            for b in ready {
                let req_url = format!("http://{}:{}/push", peeraddr.ip(), peeraddr.port());
                let req = Request::builder()
                    .method(Method::POST)
                    .uri(req_url)
                    .header("content-type", "application/octet-stream")
                    .body(Body::from(b.to_vec()))
                    .unwrap();
                if let Err(err) = client.request(req).await {
                    error!("error pushing bundle to remote: {}", err);
                    return false;
                }
                debug!("successfully sent bundle to {}", peeraddr.ip());
            }
            debug!("successfully sent {} bundles to {}", ready.len(), dest);
        } else {
            debug!("Nothing to forward.");
        }
        true
    }
}

impl std::fmt::Display for HttpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "http:{}", self.local_port)
    }
}
