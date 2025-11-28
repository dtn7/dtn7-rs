use crate::dtnconfig::DiscoveryTransport;
use crate::ipnd::mdns_discovery::spawn_mdns_discovery;
use crate::ipnd::udp_discovery::spawn_udp_discovery;
use crate::CONFIG;
use anyhow::Result;
use log::info;

/// Spawns neighbor discovery based on configured transport
pub async fn spawn_neighbour_discovery() -> Result<()> {
    let transport = CONFIG.lock().discovery_transport;

    match transport {
        DiscoveryTransport::UdpMulticast => {
            info!("Using UDP multicast for discovery");
            spawn_udp_discovery().await
        }
        DiscoveryTransport::Mdns => {
            info!("Using mDNS/DNS-SD for discovery");
            spawn_mdns_discovery().await
        }
        DiscoveryTransport::Both => {
            info!("Using both UDP multicast and mDNS for discovery");
            spawn_udp_discovery().await?;
            spawn_mdns_discovery().await
        }
    }
}
