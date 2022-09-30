use crate::cla::ConvergenceLayerAgent;
use crate::core::{DtnPeer, PeerType};
use crate::ipnd::{beacon::Beacon, services::*};
use crate::routing::RoutingNotifcation;
use crate::{peers_add, routing_notify, CLAS, CONFIG};
use crate::{peers_touch, DTNCORE};
use anyhow::Result;
use log::{debug, error, info, trace};
use socket2::{Domain, Socket, Type};
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::interval;

async fn receiver(socket: UdpSocket) -> Result<(), io::Error> {
    let mut buf: Vec<u8> = vec![0; 1024 * 64];
    let nodeid = CONFIG.lock().host_eid.clone();
    loop {
        if let Ok((size, peer)) = socket.recv_from(&mut buf).await {
            trace!("received {} bytes", size);
            let deserialized: Beacon = match serde_cbor::from_slice(&buf[..size]) {
                Ok(pkt) => pkt,
                Err(e) => {
                    error!("Deserialization of beacon failed: {}", e);
                    continue;
                }
            };

            // Creates a new peer from received beacon
            let dtnpeer = DtnPeer::new(
                deserialized.eid().clone(),
                peer.ip().into(),
                PeerType::Dynamic,
                deserialized.beacon_period(),
                deserialized.service_block().clas().clone(),
                deserialized.service_block().convert_services(),
            );
            if dtnpeer.eid == nodeid {
                debug!("Received beacon from myself, ignoring");
                continue;
            }
            if peers_add(dtnpeer) {
                info!(
                    "New peer discovered: {} @ {} (len={})",
                    deserialized.eid(),
                    peer,
                    size
                );
            } else {
                debug!(
                    "Beacon from known peer: {} @ {} (len={})",
                    deserialized.eid(),
                    peer,
                    size
                );
                // TODO: check if any fields have changed and update not only timestamp
                if let Err(e) = peers_touch(deserialized.eid().node().unwrap().as_ref()) {
                    error!("Failed to touch peer: {}", e);
                }
            }
            trace!("{}", deserialized);
            if let Err(err) = routing_notify(RoutingNotifcation::EncounteredPeer(
                deserialized.eid().clone(),
            ))
            .await
            {
                info!("Error while encountered peer notification: {}", err);
            }
        }
    }
}

async fn announcer(socket: UdpSocket, _v6: bool) {
    let mut task = interval(crate::CONFIG.lock().announcement_interval);
    loop {
        task.tick().await;

        // Start to build beacon announcement
        let eid = (*CONFIG.lock()).host_eid.clone();
        let beacon_period = if !crate::CONFIG.lock().enable_period {
            None
        } else {
            Some(crate::CONFIG.lock().announcement_interval)
        };
        let mut pkt = Beacon::with_config(eid, ServiceBlock::new(), beacon_period);
        // Get all available clas
        (*CLAS.lock())
            .iter()
            .for_each(|cla| pkt.add_cla(cla.name(), &Some(cla.port())));
        // Get all available services
        (*DTNCORE.lock())
            .service_list
            .iter()
            .for_each(|(tag, payload)| pkt.add_custom_service(*tag, payload.clone()));

        //let nodeid = format!("dtn://{}", (*DTNCORE.lock()).nodeid);
        //let addr = "127.0.0.1:3003".parse().unwrap();

        let mut destinations: HashMap<SocketAddr, u32> = HashMap::new();
        (*CONFIG.lock())
            .discovery_destinations
            .iter()
            .for_each(|(key, value)| {
                destinations.insert(key.clone().parse().unwrap(), *value);
            });
        for (destination, bsn) in destinations {
            (*CONFIG.lock()).update_beacon_sequence_number(&destination.to_string());
            pkt.set_beacon_sequence_number(bsn);

            if destination.ip().is_multicast() {
                trace!(
                    "Sending beacon\n{}\nto multicast address {}",
                    pkt,
                    destination
                );
            } else {
                trace!(
                    "Sending beacon\n{}\nto unicast address {}",
                    pkt,
                    destination
                );
            }
            match socket
                .send_to(&serde_cbor::to_vec(&pkt).unwrap(), destination)
                .await
            {
                Ok(amt) => {
                    debug!("sent announcement beacon (len={}) to {}", amt, destination);
                }
                Err(err) => error!("Sending announcement failed: {}", err),
            }
        }
    }
}
pub async fn spawn_neighbour_discovery() -> Result<()> {
    let v4 = (*CONFIG.lock()).v4;
    let v6 = (*CONFIG.lock()).v6;
    let port = 3003;
    if v4 {
        let addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
        let addr = addr.into();
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, None)?;
        socket.set_reuse_address(true)?;
        socket.bind(&addr)?;

        //required for tokio
        socket.set_nonblocking(true)?;

        // DEBUG: setup multicast on loopback to true
        socket
            .set_multicast_loop_v4(false)
            .expect("error activating multicast loop v4");
        socket.set_broadcast(true)?;
        for address in (*CONFIG.lock()).discovery_destinations.keys() {
            let addr: SocketAddr = address.parse().expect("Error parsing discovery address");
            if addr.is_ipv4() {
                if addr.ip().is_multicast() {
                    socket
                        .join_multicast_v4(
                            &addr.ip().to_string().parse()?,
                            &std::net::Ipv4Addr::new(0, 0, 0, 0),
                        )
                        .expect("error joining multicast v4 group");
                }
                info!("Configured discovery destination: {}", addr.ip());
            }
        }
        /*
        socket
            .join_multicast_v4(&"224.0.0.26".parse()?, &std::net::Ipv4Addr::new(0, 0, 0, 0))
            .expect("error joining multicast v4 group");
        */

        let socket1 = UdpSocket::from_std(socket.try_clone()?.into())?;
        let socket2 = UdpSocket::from_std(socket.try_clone()?.into())?;

        info!("Listening on {}", socket1.local_addr()?);

        tokio::spawn(receiver(socket1));

        tokio::spawn(announcer(socket2, false));
    }
    if v6 {
        let addr: SocketAddr = format!("[::1]:{}", port).parse()?;
        let addr = addr.into();
        let socket = Socket::new(Domain::IPV6, Type::DGRAM, None)?;
        socket.set_reuse_address(true)?;
        socket.set_only_v6(true)?;
        socket.bind(&addr)?;

        //required for tokio
        socket.set_nonblocking(true)?;

        // DEBUG: setup multicast on loopback to true
        socket
            .set_multicast_loop_v6(false)
            .expect("error activating multicast loop v6");

        socket.set_broadcast(true)?;

        for address in (*CONFIG.lock()).discovery_destinations.keys() {
            let addr: SocketAddr = address.parse().expect("Error while parsing IPv6 address");
            if addr.is_ipv6() {
                if addr.ip().is_multicast() {
                    socket
                        .join_multicast_v6(&addr.ip().to_string().parse()?, 0)
                        .expect("Error joining multicast v6 group");
                }
                info!("Configured discovery destination: {}", addr.ip());
            }
        }
        /*
        socket
            .join_multicast_v6(&"FF02::300".parse()?, 0)
            .expect("error joining multicast v6 group");
        */
        let socket1 = UdpSocket::from_std(socket.try_clone()?.into())?;
        let socket2 = UdpSocket::from_std(socket.try_clone()?.into())?;

        info!("Listening on {}", socket1.local_addr()?);

        tokio::spawn(receiver(socket1));
        tokio::spawn(announcer(socket2, true));
    }

    Ok(())
}
