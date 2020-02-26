use crate::cla::ConvergencyLayerAgent;
use crate::CONFIG;
use async_trait::async_trait;
use bp7::ByteBuffer;
use log::{debug, error, info};
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
    /*pub fn send_bundles(&self, addr: SocketAddr, bundles: Vec<ByteBuffer>) -> bool {
        // TODO: implement correct error handling
        // TODO: classic sending thread, tokio code would block and not complete large transmissions
        //thread::spawn(move || {
        let now = Instant::now();
        let num_bundles = bundles.len();
        let mut buf = Vec::new();
        for b in bundles {
            let mpdu = MPDU(b);
            if let Ok(buf2) = serde_cbor::to_vec(&mpdu) {
                buf.extend_from_slice(&buf2);
            } else {
                error!("MPDU encoding error!");
                return false;
            }
        }
        if let Ok(mut s1) = TcpStream::connect(&addr) {
            if s1.write_all(&buf).is_err() {
                error!("Error writing data to {}", addr);
                return false;
            }
            info!(
                "Transmission time: {:?} for {} bundles in {} bytes to {}",
                now.elapsed(),
                num_bundles,
                buf.len(),
                addr
            );
        } else {
            error!("Error connecting to remote {}", addr);
            return false;
        }
        //});
        true
    }*/
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
