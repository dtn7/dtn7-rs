# mDNS Discovery for dtn7-rs

## Overview

dtn7-rs supports **mDNS/DNS-SD** (`_dtn._udp.local.`) as an alternative discovery transport alongside UDP multicast IPND. This enables multiple DTN nodes to run on the same host without port conflicts and provides standard service discovery integration.

## Configuration

### Config File

```toml
[discovery]
interval = "2s"
peer-timeout = "20s"

# Transport options:
# - "udp-multicast" (default): IPND over UDP multicast
# - "mdns": mDNS/DNS-SD service discovery
# - "both": Run both transports simultaneously
transport = "mdns"
```

### Command Line

```bash
# Enable mDNS discovery
dtnd --discovery-transport mdns

# Use both UDP multicast and mDNS
dtnd --discovery-transport both

# Default (UDP multicast only)
dtnd --discovery-transport udp-multicast
```

## Service Structure

Each node registers an mDNS service:

- **Service Type:** `_dtn._udp.local.`
- **Instance Name:** Node ID (e.g., `node1`)
- **Hostname:** `<nodeid>.local.`
- **Port:** 0 (CLAs use their own ports)
- **TXT Records:**
  - `eid`: Endpoint ID (e.g., `dtn://node1/`)
  - `cla0`, `cla1`, ...: Available CLAs with ports (e.g., `mtcp:16162`)
  - `svc<tag>`: Custom services (same semantics as IPND)

Example service:
```
node1._dtn._udp.local
  eid=dtn://node1/
  cla0=mtcp:16162
  cla1=tcp:4556
  svc63=f2f-app-v1.0
```

## Discovery Process

1. **Registration:** Node registers `_dtn._udp.local.` service on startup
2. **Browsing:** Node browses for services of type `_dtn._udp.local.`
3. **Resolution:** Extract peer EID, CLAs, and IP address from TXT records
4. **Peer Management:** Add discovered peers to global `PEERS` map
5. **Routing:** Notify routing agent via `RoutingNotification::EncounteredPeer`
6. **Refresh:** Re-register service at `interval` to update CLA changes

## Comparison: UDP Multicast vs mDNS

| Feature | UDP Multicast | mDNS |
|---------|--------------|------|
| Port conflicts | Yes (only one daemon/host) | No (system manages mDNS) |
| Same-host discovery | No | Yes |
| Standard tooling | Limited | Full (dns-sd, avahi-browse) |
| Name resolution | No | Yes (`.local` hostnames) |
| Legacy compatibility | IPND | New |

## Platform Requirements

- **macOS:** Works out of the box (Bonjour)
- **Linux:** Requires Avahi daemon
- **Windows:** Requires Bonjour Service

## Implementation

Transport selection happens in `spawn_neighbour_discovery()`:

```rust
match CONFIG.lock().discovery_transport {
    DiscoveryTransport::UdpMulticast => spawn_udp_discovery().await,
    DiscoveryTransport::Mdns => spawn_mdns_discovery().await,
    DiscoveryTransport::Both => {
        spawn_udp_discovery().await?;
        spawn_mdns_discovery().await
    }
}
```

Dependencies:
```toml
mdns-sd = "0.11"
```

## Security Considerations

mDNS discovery inherits IPND security properties:

- No peer authentication
- No service encryption
- Local network visibility

For F2F networks, implement application-level security via custom TXT records and bundle payload encryption.
