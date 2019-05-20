# dtn7-rs
Rust implementation of a Daemon for DTN7 Bundle Protocol draft https://tools.ietf.org/html/draft-ietf-dtn-bpbis-13

Plus:
* Minimal TCP Convergence Layer Protocol https://tools.ietf.org/html/draft-ietf-dtn-mtcpcl-01
* Simple TCP Convergency Layer Protocol https://tools.ietf.org/html/draft-burleigh-dtn-stcp-00 (*DEPRECATED*)

A similar golang implementation can be found here: https://github.com/dtn7/dtn7

The actual BP7 implementation can be found here: https://github.com/gh0st42/bp7-rs

Currently a very basic service discovery, STCP/MTCP (flooding/epidemic) and a rest command interface are implemented.
**Beware, the API is not very idiomatic rust and lacks documentation and tests.**
Since I consider this code to be very unpolished and far from finished it is also not yet published on crates.io. Correct forwarding, administrative records and various other pieces are also not implemented yet. Furthermore, the rest interface is totally undocumented and unfinished :)

## Installation

```
git clone https://github.com/gh0st42/dtn7-rs
cd dtn7
cargo install --path .
```

## Usage

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

