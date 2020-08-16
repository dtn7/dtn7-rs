# dtn7-rs

[![Crates.io](https://img.shields.io/crates/v/dtn7.svg)](https://crates.io/crates/dtn7)
[![Docs.rs](https://docs.rs/dtn7/badge.svg)](https://docs.rs/dtn7)
[![Build status](https://api.travis-ci.org/dtn7/dtn7-rs.svg?branch=master)](https://travis-ci.org/dtn7/dtn7-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)

Rust implementation of a daemon for DTN7 Bundle Protocol draft https://tools.ietf.org/html/draft-ietf-dtn-bpbis-26

Plus:
* Minimal TCP Convergence Layer Protocol https://tools.ietf.org/html/draft-ietf-dtn-mtcpcl-01
* A simple HTTP Convergence Layer 
* Convenient command line tools to interact with the daemon
* A simple web interface for status information about `dtnd` 
* A web-socket interface for application agents

The actual BP7 implementation can be found here: https://github.com/dtn7/bp7-rs

Additional dtn related stuff and some client code can be found here: https://github.com/dtn7/bp7-plus-rs

A similar golang implementation can be found here: https://github.com/dtn7/dtn7-go

Currently a very basic service discovery, MTCP & HTTP CLs, flooding/epidemic/sink-routing and rest/ws command interfaces are implemented. Both addressing schemes, *dtn* as well as *ipn* are supported. Furthermore, some CLI tools are provided to easily integrate *dtn7* into shell scripts.

**Beware, the API is not always idiomatic rust and lacks documentation and tests at the moment.**

I consider this code to be work-in-progress and not finished yet. Also the rest and web-socket interface is totally undocumented and unfinished at the moment :)

## Installation

Installation from source using cargo:
```
cargo install dtn7
```

## Usage

### Daemon

```
$ dtnd -h
dtn7-rs 0.11.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Daemon for Delay Tolerant Networking

USAGE:
    dtnd [FLAGS] [OPTIONS]

FLAGS:
    -d, --debug           Set log level to debug
    -h, --help            Prints help information
    -4, --ipv4            Use IPv4
    -6, --ipv6            Use IPv6
    -U, --unsafe-httpd    Allow httpd RPC calls from anyhwere
    -V, --version         Prints version information

OPTIONS:
    -C, --cla <CLA[:local_port]>...    Add convergency layer agent: dummy, mtcp, http
    -c, --config <FILE>                Sets a custom config file
    -D, --db <STORE>                   Set bundle store: mem, sled
    -e, --endpoint <ENDPOINT>...       Registers an application agent for a node local endpoint (e.g. 'incoming' listens
                                       on 'dtn://node1/incoming')
    -i, --interval <humantime>         Sets service discovery interval (0 = deactive, 2s = 2 seconds, 3m = 3 minutes,
                                       etc.)
    -j, --janitor <humantime>          Sets janitor interval (0 = deactive, 2s = 2 seconds, 3m = 3 minutes, etc.)
    -n, --nodeid <NODEID>              Sets local node name (e.g. 'dtn://node1')
    -p, --peer-timeout <humantime>     Sets timeout to remove peer (default = 20s)
    -r, --routing <ROUTING>            Set routing algorithm: epidemic, flooding, sink
    -s, --static-peer <PEER>...        Adds a static peer (e.g. mtcp://192.168.2.1:2342/node2)
    -w, --web-port <PORT>              Sets web interface port (default = 3000)
    -W, --workdir <PATH>               Sets the working directory (e.g. '/tmp/node1', default '.')
```

Example usage for *node1* with *epidemic* routing, *mtcp* convergence layer and the default endpoint *'incoming'*:
```
$ dtnd -n node1 -r epidemic -C mtcp -e incoming
```

The same but with ipn addressing scheme and a default endpoint at *23.42*:
```
$ dtnd -n 23 -r epidemic -C mtcp -e 42
```

Configuration can also happen via a config file. 
For an example take a look at `examples/dtn7.toml.example`.

### Helpers

Querying information from `dtnd`:
```
$ dtnquery -h
dtnquery 0.11.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Query Utility for Delay Tolerant Networking

USAGE:
    dtnquery [FLAGS] [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
    -6, --ipv6       Use IPv6
    -V, --version    Prints version information

OPTIONS:
    -p, --port <PORT>    Local web port (default = 3000)

SUBCOMMANDS:
    bundles    list bundles in node
    eids       list registered endpoint IDs
    help       Prints this message or the help of the given subcommand(s)
    info       General dtnd info
    nodeid     Local node id
    peers      list known peers
    store      list bundles status in store
```

Receiving bundles: 
```
$ dtnrecv -h
dtnrecv 0.11.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Receive Utility for Delay Tolerant Networking

USAGE:
    dtnrecv [FLAGS] [OPTIONS] --endpoint <ENDPOINT>

FLAGS:
    -h, --help       Prints help information
    -x, --hex        hex output
    -6, --ipv6       Use IPv6
    -V, --version    Prints version information
    -v, --verbose    verbose output

OPTIONS:
    -b, --bundle-id <BID>        Download any bundle by ID
    -e, --endpoint <ENDPOINT>    Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/incoming'
    -o, --output <FILE>          Write bundle payload to file instead of stdout
    -p, --port <PORT>            Local web port (default = 3000)
```

Sending bundles:
```
$ dtnsend -h
dtnsend 0.11.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Send Utility for Delay Tolerant Networking

USAGE:
    dtnsend [FLAGS] [OPTIONS] --receiver <RECEIVER> [infile]

FLAGS:
    -D, --dry-run    Don't actually send packet, just dump the encoded one.
    -h, --help       Prints help information
    -6, --ipv6       Use IPv6
    -V, --version    Prints version information
    -v, --verbose    verbose output

OPTIONS:
    -l, --lifetime <SECONDS>     Bundle lifetime in seconds (default = 3600)
    -p, --port <PORT>            Local web port (default = 3000)
    -r, --receiver <RECEIVER>    Receiver EID (e.g. 'dtn://node2/incoming')
    -s, --sender <SENDER>        Sets sender name (e.g. 'dtn://node1')

ARGS:
    <infile>    File to send, if omitted data is read from stdin till EOF
```

Automatic triggering of external binaries for incoming bundles:
```
$ dtntrigger -h
dtntrigger 0.11.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Incoming Trigger Utility for Delay Tolerant Networking

USAGE:
    dtntrigger [FLAGS] [OPTIONS] --command <CMD>

FLAGS:
    -h, --help       Prints help information
    -6, --ipv6       Use IPv6
    -V, --version    Prints version information
    -v, --verbose    verbose output

OPTIONS:
    -c, --command <CMD>          Command to execute for incoming bundles, param1 = source, param2 = payload file
    -e, --endpoint <ENDPOINT>    Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/incoming'
    -p, --port <PORT>            Local web port (default = 3000)
```

### Examples

A simple DTN echo service can be found under `examples/dtnecho.rs`. 

This service automatically registers its endpoint and listens for any incoming bundles on the local `/echo` endpoint or for *ipn* addresses on service number `7`. 
Each bundle is sent back to its source with the same payload and lifetime, no delivery report is requested. 

### Acknowledging this work

If you use this software in a scientific publication, please cite the following paper:

```BibTeX
@INPROCEEDINGS{baumgaertner2019bdtn7,
  author={L. {Baumgärtner} and J. {Höchst} and T. {Meuser}},
  booktitle={2019 International Conference on Information and Communication Technologies for Disaster Management (ICT-DM)},
  title={B-DTN7: Browser-based Disruption-tolerant Networking via Bundle Protocol 7},
  year={2019},
  volume={},
  number={},
  pages={1-8},
  keywords={Protocols;Browsers;Software;Convergence;Servers;Synchronization;Wireless fidelity},
  doi={10.1109/ICT-DM47966.2019.9032944},
  ISSN={2469-8822},
  month={Dec},
}
```

### License

Licensed under either of <a href="LICENSE-APACHE">Apache License, Version 2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.


Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in `dtn7-rs` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
