# External Routing Getting Started

First, you need to enable the external routing in `dtnd`.
It's done by changing the routing strategy to ``external`` in the dtnd arguments. An example would be:

```
dtnd -w 3000 -r external -n node1
```

By default if external routing is selected the WebSocket transport layer is running under the web port of dtnd as specified by ``-w``, ``--web-port``. In the example above the port would be 3000 (``-w 3000``).

# External Routing FAQ

### What happens if no external router is connected to dtnd?

There is no fallback routing so without a connected external router no bundles will be routed.

### What happens if an external router connects while another one is already connected?

The WebSocket connection of the new external router will be closed. The already connected one stays connected. There can only be one connected router at a time!

### Is there an example implementation for external routing to look at?

A rust implementation of epidemic and flooding routing using the provided rust client can be found in:
- ``/examples/dtnerouting.rs``

A minimal python implementation of direct and first contact routing can be found in:
- ``/examples/python/dtnerouting_direct.py``
- ``/examples/python/dtnerouting_firstcontact.py``
