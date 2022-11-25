# dtn7-rs

[![Crates.io](https://img.shields.io/crates/v/dtn7.svg)](https://crates.io/crates/dtn7)
[![Docs.rs](https://docs.rs/dtn7/badge.svg)](https://docs.rs/dtn7)
[![Build status](https://api.travis-ci.org/dtn7/dtn7-rs.svg?branch=master)](https://travis-ci.org/dtn7/dtn7-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE-MIT)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)
[![Chat](https://img.shields.io/matrix/dtn7:matrix.org)](https://matrix.to/#/#dtn7:matrix.org)
[![Mastodon](https://img.shields.io/mastodon/follow/109375208547495905?domain=https%3A%2F%2Ffosstodon.org&style=social)](https://img.shields.io/mastodon/follow/109375208547495905?domain=https%3A%2F%2Ffosstodon.org&style=social)

Rust implementation of a disruption-tolerant networking (DTN) daemon for the [Bundle Protocol version 7 - RFC9171](https://datatracker.ietf.org/doc/rfc9171/).

Plus:
* [TCP Convergence Layer v4 - RFC9174](https://datatracker.ietf.org/doc/rfc9174/)
* [Minimal TCP Convergence Layer](https://tools.ietf.org/html/draft-ietf-dtn-mtcpcl-01) 
* A simple HTTP Convergence Layer 
* An IP neighorhood discovery service
* Convenient command line tools to interact with the daemon
* A simple web interface for status information about `dtnd` 
* A [web-socket interface](doc/http-client-api.md) for application agents
* Interfaces for external processes to provide [routing strategies](doc/erouting.md) and [convergence layers](doc/ecla.md)

The actual BP7 implementation (encoding/decoding) is available as a separate [project](https://github.com/dtn7/bp7-rs).

Additional dtn extensions and a client library are also [available](https://crates.io/crates/dtn7-plus).

Currently, a service discovery based on IPND but adapted to CBOR and BPv7, TCP, MTCP & HTTP CLs, sprayandwait/flooding/epidemic/sink-routing and restful/websocket command interfaces are implemented. 
Both addressing schemes, *dtn* as well as *ipn* are supported. 
Furthermore, some CLI tools are provided to easily integrate *dtn7* into shell scripts.

**Beware: This code is still under development and thus might change frequently.**


## Installation

Installation from source using cargo:
```
cargo install --bins --examples dtn7
```

In case of compilation issues, try adding `--locked` to the command. 
This will use the exact versions of all dependencies as used in our repository.

Precompiled binaries for common platforms can be found on [GitHub](https://github.com/dtn7/dtn7-rs/releases).

## Usage

In the following some of the commands shipped with dtn7 are listed.
There is also a more in-depth [getting started guide](doc/getting-started.md) available.

### Daemon

The main *Bundle Protocol Agent* `dtnd` can be configured either through the CLI (`dtnd --help`) or by providing a [configuration file](examples/dtn7.toml.example).
Command line options override configuration file settings if both are mixed. 
The daemon does not fork into background but can be easily started as a background service by invoking it with the `nohup` command and `&`.

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

- `dtnquery`: Querying information from `dtnd` such as *peers*, *bundles*, *nodeid*, etc.
- `dtnrecv`: A simple tool to check for new bundles on a specific endpoint, can be used for scripting.
- `dtnsend`: A simple tool to send a bundle from a provided file or pipe, can be used for scripting.
- `dtntrigger`: Automatic triggering of external binaries for incoming bundles, useful for advanced scripting.

### Example Applications

A simple DTN echo service can be found under `examples/dtnecho2.rs`. 

This service automatically registers its endpoint and listens for any incoming bundles on the local `/echo` endpoint or for *ipn* addresses on service number `7`. 
Each bundle is sent back to its source with the same payload and lifetime, no delivery report is requested. 

This service can be used together with `examples/dtnping.rs` for connectivity tests.

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
