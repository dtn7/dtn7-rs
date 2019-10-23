# dtn7-rs

[![Crates.io](https://img.shields.io/crates/v/dtn7.svg)](https://crates.io/crates/dtn7)
[![Docs.rs](https://docs.rs/dtn7/badge.svg)](https://docs.rs/dtn7)
[![Build status](https://api.travis-ci.org/gh0st42/dtn7-rs.svg?branch=master)](https://travis-ci.org/gh0st42/dtn7-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)

Rust implementation of a Daemon for DTN7 Bundle Protocol draft https://tools.ietf.org/html/draft-ietf-dtn-bpbis-13

Plus:
* Minimal TCP Convergence Layer Protocol https://tools.ietf.org/html/draft-ietf-dtn-mtcpcl-01

A similar golang implementation can be found here: https://github.com/dtn7/dtn7-go

The actual BP7 implementation can be found here: https://github.com/dtn7/bp7-rs

Currently a very basic service discovery, MTCP (flooding/epidemic) and a rest command interface are implemented.
**Beware, the API is not very idiomatic rust and lacks documentation and tests.**
I consider this code to be very unpolished and far from finished. Correct forwarding, administrative records and various other pieces are also not much tested yet. Furthermore, the rest interface is totally undocumented and unfinished :)

## Installation

```
git clone https://github.com/dtn7/dtn7-rs
cd dtn7
cargo install --path .
```

## Usage

### Daemon

```
$ dtnd -h
dtn7-rs 0.3.1
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Daemon for Delay Tolerant Networking

USAGE:
    dtnd [FLAGS] [OPTIONS]

FLAGS:
    -d, --debug      Set log level to debug
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -C, --cla <CLA>...              Add convergency layer agent: dummy, stcp, mtcp
    -c, --config <FILE>             Sets a custom config file
    -e, --endpoint <ENDPOINT>...    Registers an application agent for a node local endpoint (e.g. 'incoming' listens on
                                    'dtn://node1/incoming')
    -i, --interval <INTERVAL>       Sets service discovery interval
    -j, --janitor <INTERVAL>        Sets janitor interval
    -n, --nodeid <NODEID>           Sets local node name (e.g. 'dtn://node1')
    -p, --peer-timeout <SECONDS>    Sets timeout to remove peer
    -r, --routing <ROUTING>         Set routing algorithm: flooding, epidemic
    -s, --static-peer <PEER>...     Adds a static peer (e.g. stcp://192.168.2.1/node2)
```

### Helpers

```
$ dtnquery -h
dtnquery 0.4.0
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
dtnsend 0.4.0
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