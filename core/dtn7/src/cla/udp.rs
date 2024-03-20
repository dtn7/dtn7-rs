use crate::cla::{ConvergenceLayerAgent, TransferResult};
use async_trait::async_trait;
use bp7::{Bundle, ByteBuffer};
use core::convert::TryFrom;
use dtn7_codegen::cla;
use log::{debug, error, info};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::time::Instant;
use tokio::io;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use super::HelpStr;

async fn udp_listener(addr: String, port: u16) -> Result<(), io::Error> {
    let addr: SocketAddrV4 = format!("{}:{}", addr, port).parse().unwrap();
    let listener = UdpSocket::bind(&addr)
        .await
        .expect("failed to bind udp port");
    debug!("spawning UDP listener on port {}", port);
    loop {
        let mut buf = [0; 65535];
        let (amt, src) = listener.recv_from(&mut buf).await?;
        let buf = &buf[..amt];
        if let Ok(bndl) = Bundle::try_from(buf.to_vec()) {
            info!("Received bundle: {} from {}", bndl.id(), src);
            {
                tokio::spawn(async move {
                    if let Err(err) = crate::core::processing::receive(bndl).await {
                        error!("Failed to process bundle: {}", err);
                    }
                });
            }
        } else {
            crate::STATS.lock().broken += 1;
            info!("Error decoding bundle from {}", src);
        }
    }
}

pub async fn udp_send_bundles(addr: SocketAddr, bundles: Vec<ByteBuffer>) -> TransferResult {
    let now = Instant::now();
    let num_bundles = bundles.len();
    let total_bytes: usize = bundles.iter().map(|b| b.len()).sum();

    let sock = UdpSocket::bind("0.0.0.0:0").await;
    if sock.is_err() {
        error!("Error binding UDP socket for sending");
        return TransferResult::Failure;
    }
    let sock = sock.unwrap();
    if sock.connect(addr).await.is_err() {
        error!("Error connecting UDP socket for sending");
        return TransferResult::Failure;
    }

    for b in bundles {
        if b.len() > 65535 {
            error!("Bundle too large for UDP transmission");
            return TransferResult::Failure;
        }
        // send b via udp socket to addr
        if sock.send(&b).await.is_err() {
            error!("Error sending bundle via UDP");
            return TransferResult::Failure;
        }
    }

    debug!(
        "Transmission time: {:?} for {} bundles in {} bytes to {}",
        now.elapsed(),
        num_bundles,
        total_bytes,
        addr
    );

    TransferResult::Successful
}

#[cla(udp)]
#[derive(Debug, Clone)]
pub struct UdpConvergenceLayer {
    local_addr: String,
    local_port: u16,
    tx: mpsc::Sender<super::ClaCmd>,
}

impl UdpConvergenceLayer {
    pub fn new(local_settings: Option<&HashMap<String, String>>) -> UdpConvergenceLayer {
        let addr: String = local_settings
            .and_then(|settings| settings.get("bind"))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());
        let port = local_settings
            .and_then(|settings| settings.get("port"))
            .and_then(|port_str| port_str.parse::<u16>().ok())
            .unwrap_or(4556);
        let (tx, mut rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    super::ClaCmd::Transfer(remote, data, reply) => {
                        debug!(
                            "UdpConvergenceLayer: received transfer command for {}",
                            remote
                        );
                        if !data.is_empty() {
                            let peeraddr: SocketAddr = remote.parse().unwrap();
                            debug!("forwarding to {:?}", peeraddr);
                            tokio::spawn(async move {
                                reply
                                    .send(udp_send_bundles(peeraddr, vec![data]).await)
                                    .unwrap();
                            });
                        } else {
                            debug!("Nothing to forward.");
                            reply.send(TransferResult::Successful).unwrap();
                        }
                    }
                    super::ClaCmd::Shutdown => {
                        debug!("UdpConvergenceLayer: received shutdown command");
                        break;
                    }
                }
            }
        });
        UdpConvergenceLayer {
            local_addr: addr,
            local_port: port,
            tx,
        }
    }

    pub async fn spawn_listener(&self) -> std::io::Result<()> {
        // TODO: bubble up errors from run
        tokio::spawn(udp_listener(self.local_addr.clone(), self.local_port)); /*.await.unwrap()*/
        Ok(())
    }
}

#[async_trait]
impl ConvergenceLayerAgent for UdpConvergenceLayer {
    async fn setup(&mut self) {
        self.spawn_listener()
            .await
            .expect("error setting up udp listener");
    }
    fn port(&self) -> u16 {
        self.local_port
    }
    fn name(&self) -> &str {
        "udp"
    }
    fn channel(&self) -> tokio::sync::mpsc::Sender<super::ClaCmd> {
        self.tx.clone()
    }
}

impl HelpStr for UdpConvergenceLayer {
    fn local_help_str() -> &'static str {
        "port=4556:bind=0.0.0.0"
    }
}
impl std::fmt::Display for UdpConvergenceLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "udp:{}:{}", self.local_addr, self.local_port)
    }
}
