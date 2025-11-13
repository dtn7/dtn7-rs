# ECLA Getting Started

First, you need to enable the ECLA in dtnd. It's done by adding the ``--ecla`` flag to the dtnd arguments. An example would be:

```
dtnd -w 3000 -r epidemic -n node1 --ecla
```

By default, if ECLA is enabled, the WebSocket transport layer is running under the web port of dtnd as specified by ``-w``, ``--web-port``.
In the example above the port would be 3000 (``-w 3000``).
(If the TCP transport layer for connecting ECLAs is desired, it can be activated via setting the ``--ecla-tcp XYZ`` flag.)

## Example Config File

```toml
nodeid = "node1"
debug = false
beacon-period = true
generate-status-reports = false
parallel-bundle-processing = false
webport = 3000
workdir = "/tmp/dtn7"
db = "mem"

[routing]
strategy = "epidemic"

[core]
janitor = "10s"

[discovery]
interval = "2s"
peer-timeout = "20s"

[ecla]
enabled  = true
tcp_port = 0
```


# ECLA FAQ

### What happens if a ECLA connects with a name that is already present in the dtnd as CLA?

The registration process fails with a ``already registered`` error packet as a response and the connection will be closed by dtnd.

### Where does my WebSocket client need to connect to?

The WebSocket is accessible under the same port as defined by ``-w``, ``--web-port`` and the route ``/ws/ecla``. An example for a web port 3000 would be ``127.0.0.1:3000/ws/ecla``.

### Is there an example implementation for external CLAs to look at?

A rust implementation of MTCP and JSON based MTCP using the provided rust client can be found in:
- ``/examples/dtnecla_mtcp.rs``
- ``/examples/dtnecla_json_mtcp.rs``
