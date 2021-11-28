# dtn7-rs

[![Crates.io](https://img.shields.io/crates/v/dtn7.svg)](https://crates.io/crates/dtn7)
[![Docs.rs](https://docs.rs/dtn7/badge.svg)](https://docs.rs/dtn7)
[![Build status](https://api.travis-ci.org/dtn7/dtn7-rs.svg?branch=master)](https://travis-ci.org/dtn7/dtn7-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)

Rust implementation of a disruption-tolerant networking (DTN) daemon for the [Bundle Protocol version 7 draft](https://tools.ietf.org/html/draft-ietf-dtn-bpbis).

Plus:
* [TCP Convergence Layer v4](https://datatracker.ietf.org/doc/html/draft-ietf-dtn-tcpclv4)
* [Minimal TCP Convergence Layer](https://tools.ietf.org/html/draft-ietf-dtn-mtcpcl-01) 
* A simple HTTP Convergence Layer 
* An IP neighorhood discovery service
* Convenient command line tools to interact with the daemon
* A simple web interface for status information about `dtnd` 
* A [web-socket interface](doc/http-client-api.md) for application agents

The actual BP7 implementation (encoding/decoding) is available as a separate [project](https://github.com/dtn7/bp7-rs).

Additional dtn extensions and a client library are also [available](https://github.com/dtn7/bp7-plus-rs).

Currently, a service discovery based on IPND but adapted to CBOR and BPv7, TCP, MTCP & HTTP CLs, flooding/epidemic/sink-routing and rest/ws command interfaces are implemented. Both addressing schemes, *dtn* as well as *ipn* are supported. Furthermore, some CLI tools are provided to easily integrate *dtn7* into shell scripts.

**Beware: This code as well as the RFC drafts are not yet final and thus might change frequently.**

I consider this code to be work-in-progress and not finished yet. 

## Installation

Installation from source using cargo:
```
cargo install dtn7
```

Precompiled binaries for common platforms can be found on [GitHub](https://github.com/dtn7/dtn7-rs/releases).

## Usage

In the following some of the commands shipped with dtn7 are listed.
There is also a more in-depth [getting started guide](doc/getting-started.md) available.

### Daemon

```
$ dtnd -h
dtn7-rs 0.17.1
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Daemon for Delay Tolerant Networking

USAGE:
    dtnd [FLAGS] [OPTIONS]

FLAGS:
    -b, --beacon-period              Enables the advertisement of the beacon sending interval to inform neighbors about
                                     when to expect new beacons
    -d, --debug                      Set log level to debug
    -g, --generate-status-reports    Generate status report bundles, can lead to a lot of traffic (default: deactivated)
    -h, --help                       Prints help information
    -4, --ipv4                       Use IPv4
    -6, --ipv6                       Use IPv6
    -U, --unsafe-httpd               Allow httpd RPC calls from anyhwere
    -V, --version                    Prints version information

OPTIONS:
    -C, --cla <CLA[:local_port]>...               Add convergence layer agent: dummy, mtcp, http, tcp
    -c, --config <FILE>                           Sets a custom config file
    -D, --db <STORE>                              Set bundle store: mem, sled, sneakers
    -E, --discovery-destination <DD[:port]>...
            Sets destination beacons shall be sent to for discovery purposes (default IPv4 = 224.0.0.26:3003, IPv6 =
            [FF02::300]:3003
    -e, --endpoint <ENDPOINT>...
            Registers an application agent for a node local endpoint (e.g. 'incoming' listens on 'dtn://node1/incoming')

    -i, --interval <humantime>
            Sets service discovery interval (0 = deactive, 2s = 2 seconds, 3m = 3 minutes, etc.) Refers to the discovery
            interval that is advertised when flag -b is set
    -j, --janitor <humantime>
            Sets janitor interval (0 = deactive, 2s = 2 seconds, 3m = 3 minutes, etc.)

    -n, --nodeid <NODEID>                         Sets local node name (e.g. 'dtn://node1')
    -p, --peer-timeout <humantime>                Sets timeout to remove peer (default = 20s)
    -r, --routing <ROUTING>                       Set routing algorithm: epidemic, flooding, sink
    -S, --service <TAG:payload>...                Add a self defined service.
    -s, --static-peer <PEER>...                   Adds a static peer (e.g. mtcp://192.168.2.1:2342/node2)
    -w, --web-port <PORT>                         Sets web interface port (default = 3000)
    -W, --workdir <PATH>                          Sets the working directory (e.g. '/tmp/node1', default '.')
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
For an example take a look at [examples/dtn7.toml.example](examples/dtn7.toml.example).

### Helpers

Querying information from `dtnd`:
```
$ dtnquery -h
dtnquery 0.17.1
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
dtnrecv 0.17.1
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
    -e, --endpoint <ENDPOINT>    Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/~incoming'
    -o, --output <FILE>          Write bundle payload to file instead of stdout
    -p, --port <PORT>            Local web port (default = 3000)
```

Sending bundles:
```
$ dtnsend -h
dtnsend 0.17.0
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
dtntrigger 0.17.1
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
    -e, --endpoint <ENDPOINT>    Specify local endpoint, e.g. '/incoming', or a group endpoint 'dtn://helpers/~incoming'
    -p, --port <PORT>            Local web port (default = 3000)
```

### Example Applications

A simple DTN echo service can be found under `examples/dtnecho2.rs`. 

This service automatically registers its endpoint and listens for any incoming bundles on the local `/echo` endpoint or for *ipn* addresses on service number `7`. 
Each bundle is sent back to its source with the same payload and lifetime, no delivery report is requested. 

This service can be used together with `examples/dtnping.rs` for connectivity tests.
```
dtnping 0.17.1
Lars Baumgaertner <baumgaertner@cs.tu-darmstadt.de>
A simple Bundle Protocol 7 Ping Tool for Delay Tolerant Networking

USAGE:
    dtnping [FLAGS] [OPTIONS] --destination <destination>

FLAGS:
    -h, --help       Prints help information
    -6, --ipv6       Use IPv6
    -V, --version    Prints version information
    -v, --verbose    verbose output

OPTIONS:
    -c, --count <count>                Number of pings to send
    -d, --destination <destination>    Destination to ping
    -s, --size <payloadsize>           Payload size
    -p, --port <PORT>                  Local web port (default = 3000)
    -t, --timeout <timeout>            Time to wait for reply (10s, 30m, 2h, ...)
```

### Example Use-Cases

Under `tests/` are several shell scripts for integration tests that also showcase how to use the different command line utilities. 
Furthermore, under `tests/clab` are more complex and dynamic tests that get executed in *Docker* and [*coreemu*](https://github.com/coreemu/core).

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
