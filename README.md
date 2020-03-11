# dtn7-rs

[![Crates.io](https://img.shields.io/crates/v/dtn7.svg)](https://crates.io/crates/dtn7)
[![Docs.rs](https://docs.rs/dtn7/badge.svg)](https://docs.rs/dtn7)
[![Build status](https://api.travis-ci.org/dtn7/dtn7-rs.svg?branch=master)](https://travis-ci.org/gh0st42/dtn7-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)

Rust implementation of a daemon for DTN7 Bundle Protocol draft https://tools.ietf.org/html/draft-ietf-dtn-bpbis-23

Plus:
* Minimal TCP Convergence Layer Protocol https://tools.ietf.org/html/draft-ietf-dtn-mtcpcl-01
* A simple HTTP Convergence Layer 
* Convenient command line tools to interact with the daemon
* A simple web interface for status information about `dtnd` 

A similar golang implementation can be found here: https://github.com/dtn7/dtn7-go

The actual BP7 implementation can be found here: https://github.com/dtn7/bp7-rs

Currently a very basic service discovery, MTCP & HTTP CLs, flooding/epidemic/sink-routing and a rest command interface are implemented. Both addressing schemes, *dtn* as well as *ipn* are supported. 

**Beware, the API is not always idiomatic rust and lacks documentation and tests at the moment.**

I consider this code to be work-in-progress and not finished yet. Also the rest interface is totally undocumented and unfinished at the moment:)

## Installation

```
cargo install dtn7
```

## Usage

### Daemon

```
$ dtnd -h
dtn7-rs 0.6.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Daemon for Delay Tolerant Networking

USAGE:
    dtnd [FLAGS] [OPTIONS]

FLAGS:
    -d, --debug      Set log level to debug
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -C, --cla <CLA[:local_port]>...    Add convergency layer agent: dummy, mtcp, http
    -c, --config <FILE>                Sets a custom config file
    -e, --endpoint <ENDPOINT>...       Registers an application agent for a node local endpoint (e.g. 'incoming' listens
                                       on 'dtn://node1/incoming')
    -i, --interval <MS>                Sets service discovery interval (0 = deactive)
    -j, --janitor <MS>                 Sets janitor interval (0 = deactive)
    -n, --nodeid <NODEID>              Sets local node name (e.g. 'dtn://node1')
    -p, --peer-timeout <SECONDS>       Sets timeout to remove peer
    -r, --routing <ROUTING>            Set routing algorithm: epidemic, flooding, sink
    -s, --static-peer <PEER>...        Adds a static peer (e.g. mtcp://192.168.2.1:2342/node2)
    -w, --web-port <PORT>              Sets web interface port (default = 3000)
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

```
$ dtnquery -h
dtnquery 0.6.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Query Utility for Delay Tolerant Networking

USAGE:
    dtnquery [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help       Prints help information
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

$ dtnrecv -h
dtnrecv 0.4.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Receive Utility for Delay Tolerant Networking

USAGE:
    dtnrecv [FLAGS] [OPTIONS] --endpoint <ENDPOINT>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    verbose output

OPTIONS:
    -e, --endpoint <ENDPOINT>    Specify local endpoint, e.g. '/incoming')
    -o, --output <FILE>          Write bundle payload to file instead of stdout
    -p, --port <PORT>            Local web port (default = 3000)
   
$ dtnsend -h
dtnsend 0.6.0
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Send Utility for Delay Tolerant Networking

USAGE:
    dtnsend [FLAGS] [OPTIONS] --receiver <RECEIVER> [infile]

FLAGS:
    -D, --dry-run    Don't actually send packet, just dump the encoded one.
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v, --verbose    verbose output

OPTIONS:
    -p, --port <PORT>            Local web port (default = 3000)
    -r, --receiver <RECEIVER>    Receiver EID (e.g. 'dtn://node2/incoming')
    -s, --sender <SENDER>        Sets sender name (e.g. 'dtn://node1')

ARGS:
    <infile>    File to send, if omitted data is read from stdin till EOF
```

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version 2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in dtn7-rs by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
</sub>