use crate::core::{DtnPeer, PeerType};
use crate::ipnd::{beacon::Beacon, services::*};
use crate::routing::RoutingNotifcation;
use crate::DTNCORE;
use crate::{peers_add, routing_notify, CONFIG};
use anyhow::Result;
use log::{debug, error, info};
use net2::UdpBuilder;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::interval;

struct Server {
    socket: UdpSocket,
    buf: Vec<u8>,
}

impl Server {
    async fn run(self) -> Result<(), io::Error> {
        let Server {
            mut socket,
            mut buf,
        } = self;

        loop {
            // Server received a Beacon
            if let Some((size, peer)) = Some(socket.recv_from(&mut buf).await?) {
                debug!("Beacon received");
                let deserialized: Beacon = match serde_cbor::from_slice(&buf[..size]) {
                    Ok(pkt) => pkt,
                    Err(e) => {
                        error!("Deserialization of Beacon failed!{}", e);
                        continue;
                    }
                };
                //let amt = try_ready!(self.socket.poll_send_to(&self.buf[..size], &peer));
                //println!("Echoed {}/{} bytes to {}", amt, size, peer);
                info!("Beacon from {}", peer);
                debug!(":\n{}", deserialized);
                // Creates a new peer from received beacon
                let dtnpeer = DtnPeer::new(
                    deserialized.eid().clone(),
                    peer.ip(),
                    PeerType::Dynamic,
                    deserialized.beacon_period().clone(),
                    deserialized.service_block().clas().clone(),
                    deserialized.service_block().convert_services(),
                );
                peers_add(dtnpeer);
                routing_notify(RoutingNotifcation::EncounteredPeer(&deserialized.eid()))
            }
        }
    }
}

async fn announcer(socket: std::net::UdpSocket, _v6: bool) {
    let mut task = interval(crate::CONFIG.lock().announcement_interval);
    let mut sock = UdpSocket::from_std(socket).unwrap();
    loop {
        task.tick().await;
        debug!("running announcer");

        // Start to build beacon announcement
        let eid = (*CONFIG.lock()).host_eid.clone();

        let beacon_period = if !crate::CONFIG.lock().enable_period {
            None
        } else {
            Some(crate::CONFIG.lock().announcement_interval)
        };
        let mut pkt = Beacon::with_config(eid, ServiceBlock::new(), beacon_period);

        // Get all available clas
        &(*DTNCORE.lock())
            .cl_list
            .iter()
            .for_each(|cla| pkt.add_cla(&cla.name().to_string(), &Some(cla.port())));

        // Get all available services
        &(*DTNCORE.lock())
            .service_list
            .iter()
            .for_each(|(tag, payload)| pkt.add_custom_service(*tag, payload.clone()));

        //let nodeid = format!("dtn://{}", (*DTNCORE.lock()).nodeid);
        //let addr = "127.0.0.1:3003".parse().unwrap();

        let mut destinations: HashMap<SocketAddr, u32> = HashMap::new();

        &(*CONFIG.lock())
            .discovery_destinations
            .iter()
            .for_each(|(key, value)| {
                destinations.insert(key.clone().parse().unwrap(), *value);
            });

        for (destination, bsn) in destinations {
            &(*CONFIG.lock()).update_beacon_sequence_number(&destination.to_string());
            pkt.set_beacon_sequence_number(bsn);

            if destination.ip().is_multicast() {
                debug!(
                    "Sending beacon\n{}\nto multicast address {}",
                    pkt, destination
                );
            } else {
                debug!(
                    "Sending beacon\n{}\nto unicast address {}",
                    pkt, destination
                );
            }

            if let Err(err) = sock
                .send_to(&serde_cbor::to_vec(&pkt).unwrap(), destination)
                .await
            {
                error!("Sending announcement failed: {}", err);
            }
        }
    }
}
pub async fn spawn_neighbour_discovery() -> Result<()> {
    let v4 = (*CONFIG.lock()).v4;
    let v6 = (*CONFIG.lock()).v6;
    let port = 3003;
    if v4 {
        let addr: std::net::SocketAddr = format!("0.0.0.0:{}", port).parse()?;
        let socket = UdpBuilder::new_v4()?;
        socket.reuse_address(true)?;
        let socket = socket.bind(addr)?;

        // DEBUG: setup multicast on loopback to true
        socket
            .set_multicast_loop_v4(false)
            .expect("error activating multicast loop v4");
        for (address, _bsn) in &(*CONFIG.lock()).discovery_destinations {
            let addr: SocketAddr = address.parse().expect("Error parsing discovery address");
            if addr.is_ipv4() && addr.ip().is_multicast() {
                socket
                    .join_multicast_v4(
                        &addr.ip().to_string().parse()?,
                        &std::net::Ipv4Addr::new(0, 0, 0, 0),
                    )
                    .expect("error joining multicast v4 group");
            }
        }
        /*
        socket
            .join_multicast_v4(&"224.0.0.26".parse()?, &std::net::Ipv4Addr::new(0, 0, 0, 0))
            .expect("error joining multicast v4 group");
        */
        let socket_clone = socket.try_clone()?;
        let sock = UdpSocket::from_std(socket)?;

        info!("Listening on {}", sock.local_addr()?);
        let server = Server {
            socket: sock,
            buf: vec![0; 1024],
        };
        tokio::spawn(server.run());

        tokio::spawn(announcer(
            socket_clone.try_clone().expect("couldn't clone the socket"),
            false,
        ));
    }
    if v6 {
        let addr: std::net::SocketAddr = format!("[::1]:{}", port).parse()?;
        let socket = UdpBuilder::new_v6()?;
        socket.reuse_address(true)?;
        socket.only_v6(true)?;
        let socket = socket.bind(addr)?;
        // DEBUG: setup multicast on loopback to true
        socket
            .set_multicast_loop_v6(false)
            .expect("error activating multicast loop v6");

        for (address, _bsn) in &(*CONFIG.lock()).discovery_destinations {
            let addr: SocketAddr = address.parse().expect("Error while parsing IPv6 address");
            if addr.is_ipv6() && addr.ip().is_multicast() {
                socket
                    .join_multicast_v6(&addr.ip().to_string().parse()?, 0)
                    .expect("Error joining multicast v6 group");
            }
        }
        /*
        socket
            .join_multicast_v6(&"FF02::300".parse()?, 0)
            .expect("error joining multicast v6 group");
        */
        let socket_clone = socket.try_clone()?;
        let sock = UdpSocket::from_std(socket)?;

        info!("Listening on {}", sock.local_addr()?);
        let server = Server {
            socket: sock,
            buf: vec![0; 1024],
        };
        tokio::spawn(server.run());

        tokio::spawn(announcer(
            socket_clone.try_clone().expect("couldn't clone the socket"),
            true,
        ));
    }

    Ok(())
}
