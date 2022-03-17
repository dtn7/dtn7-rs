# External Routing Getting Started

# External Routing FAQ

### What happens if no ERouter is connected to dtnd?

There is no fallback routing so without a connected external router no bundles will be routed.

### What happens if a ERouter connects while another one is already connected?

The websocket connection of the new ERouter will be closed. The already connected one stays connected.