use crate::cla::ConvergencyLayerAgent;
use crate::CONFIG;
use async_trait::async_trait;
use bp7::ByteBuffer;
use log::debug;
use std::net::SocketAddr;

#[derive(Debug, Clone, Default, Copy)]
pub struct HttpConversionLayer {
    counter: u64,
    local_port: u16,
}

impl HttpConversionLayer {
    pub fn new(port: Option<u16>) -> HttpConversionLayer {
        HttpConversionLayer {
            counter: 0,
            local_port: port.unwrap_or((*CONFIG.lock()).webport),
        }
    }
}

#[async_trait]
impl ConvergencyLayerAgent for HttpConversionLayer {
    async fn setup(&mut self) {}
    fn port(&self) -> u16 {
        self.local_port
    }
    fn name(&self) -> &'static str {
        "http"
    }
    fn scheduled_submission(&self, dest: &str, ready: &[ByteBuffer]) -> bool {
        debug!("Scheduled HTTP submission: {:?}", dest);
        if !ready.is_empty() {
            let peeraddr: SocketAddr = dest.parse().unwrap();
            debug!("forwarding to {:?}", peeraddr);
            for b in ready {
                if let Ok(_res) = attohttpc::post(&format!(
                    "http://{}:{}/push",
                    peeraddr.ip(),
                    peeraddr.port()
                ))
                .bytes(b.to_vec())
                .send()
                {
                } else {
                    return false;
                }
            }
        } else {
            debug!("Nothing to forward.");
        }
        true
    }
}

impl std::fmt::Display for HttpConversionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "http:{}", self.local_port)
    }
}
