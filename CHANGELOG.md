# Changelog
All notable changes to this project will be documented in this file.

## [0.19.0] - 2023-04-09

### Bug Fixes

- Added missing emission of Registered packet in ecla websocket client. (#49)

### Documentation

- Added mastodon link to README.md
- Added some documentation explaining the simple HTTP CL

### Features

- Added HTTP endpoints to request a hash digest of the bundles known to the local dtnd instance
- Http pull convergence layer (#44)
- Added delete endpoint to rest interface, dtnrecv can now also remove bundles (#50)

### Testing

- Coreemu-lab script now pins docker images version and always uses cross to build musl binaries

## [0.18.2] - 2022-11-24

### Bug Fixes

- Added lifetime expiration functionality in process_bundles, prior to actual forwarding
- Localendpoint bundles now also expire if not consumed by an application

### Miscellaneous Tasks

- Fixed autodef for global locks as suggested by clippy
- Updated to most recent version of clap and attohttpc

## [0.18.1] - 2022-10-09

### Bug Fixes

- Fixed minor shell scripting bugs in test scripts
- Removed debug symbols from release profile and enabled stripping binaries
- Filtering bundles from store by address no longer returns deleted BIDs (#40)
- Ignore beacons from self for broadcast IPND packets

### Documentation

- Updated README, removed tool help output and described recent feature additions.
- Added badge with link to dtn7 matrix space to README
- Changed help output regarding local endpoints in dtnrecv and dtntrigger tools

### Features

- CLAs can now indicate if the are accepting new bundles, e.g., useful for discovery only CLAs
- Support for sprayandwait routing strategy (#29)
- Added support for broadcast sockets
- Added warning if no CLA is configured

### Miscellaneous Tasks

- Bumped minimum ubuntu version in cd.yml to 20.04
- Pleased clippy of rust 1.64

### Refactor

- Reexport all external client relevant pieces of code to dtn7::client

### Testing

- Added clab scenario with two subnets

## [0.18.0] - 2022-07-21

### Bug Fixes

- Made application agent less verbose for logging
- Spawn tasks for actual CLA transmission in actor instead of blocking
- Tcpcl now spawns a new tokio task for new connections
- Seeing an already known peer did not update its last seen timestamp, now it gets touched on any new beacon
- Tcpcl now uses buffered IO
- Fixed bug where touching of known peers used wrong node ID
- Removed unneeded timeout tick from TCP CLA
- Handle all IO related unwraps in tcpcl
- Made websocket mutexes async
- Potential deadlock in forwarding code when reporting failed transmissions to routing agent fixed
- Added else case to select in tcpcl, logging and aborting session when all channels closed
- Fixed a race condition where the same bundle received multiple times in parallel also get delivery to the local endpoint multiple times. additionalls dispatching spawns a task for forwarding to return earlier.
- Check node and service names for validity with more options than just alphanumeric chars, now following URL and hostname rules
- Report decoding error to client in websocket error message
- Bundles created via WS only consist of primary block and payload, no more hopcount block

### Documentation

- Added curl example on how to use HTTP push endpoint to the HTTP client API documentation
- Add new bundle verbose and filtered endpoints to http client API documentation

### Features

- Increased sending channel buffer from 1 to 100 for http, mtcp and tcp
- ClaSenderTask now carries the next_hop node ID of the peer
- Http cla now has a timeout to complete the bundle delivery (default: 5s)
- Added transmission time output to tcpcl
- Remove peers from neighborhood list if they fail too often when transferring bundles
- Added optional feature for tokio tracing
- Increased sending channel buffer from 1 to 100 for http, mtcp and tcp
- ClaSenderTask now carries the next_hop node ID of the peer
- Transmission time debug output for all CLAs
- Add connecting peer to peer database for tcpcl if not already known and update last seen field on keep alive packets
- Added deadlock detection for tracing builds or when deadlock_detection feature is enabled
- HTTP cla now uses shared hyper client for connection pooling
- Added config to enable parallel processing of bundles - can speed up bundle transmission time but can also cause congestion and higher CPU load
- Added receive processing time to HTTP push endpoint
- Added rest endpoint with verbose bundle output and filter query interface
- Add new verbose and filter functions to dtnquery bundle CLI tool
- Add more fine granular logging to dtnd processing
- Added python example to produce message flood via websocket (bulk and sequential)
- Return bundle ID for newly sent bundle via websocket
- Ws-flooder.py now supports different payload sizes
- External CLA and Routing (#17)
- Mtcp and tcp can now be configured to bind to a specific IP address (#27)

### Miscellaneous Tasks

- Updated to newest bp7 crate, removing dbg! output
- Minor code cleanups
- Eliminated some clones
- Eliminated bundle clone in tcp CLA
- Cleanup of some log output
- Minor changes to please clippy
- Upgraded dependencies and pleased new clippy lints
- Bump crossbeam-utils from 0.7.2 to 0.8.8 in /core/dtn7/fuzz
- Bump generic-array from 0.12.3 to 0.12.4 in /core/dtn7/fuzz
- Bump regex from 1.3.9 to 1.5.6 in /core/dtn7/fuzz
- Bump miow from 0.2.1 to 0.2.2 in /core/dtn7/fuzz
- Moved fuzz project to top and updated deps to most recent versions
- Cleaned up flooding example and updated comments
- Updated dependencies and switched to dtn7-plus 0.7.0

### Refactor

- Switched ClaSender to a channel for sending
- Direct delivery priority is now up to the routing agent
- Switched ClaSender to a channel for sending
- Cleanup in forward of processing logic
- Let tcp_send_bundles directly send reply to provided oneshot address
- Refactored tcpcl to use less tasks
- Moved error handling in tcpcl connect method up to spawning task
- Made return value of cla send function an enum instead of bool

### Testing

- Local nodes http example
- Local nodes http example
- Local ping echo test now does not start a CLA
- Added coreemu-lab scenario with 3 nodes and message flooding
- Refactored tests to use more ergonimic shell functions from `libshelltests.sh` to reduce boilerplate

### Example

- Update example config to include parallel bundle processing config option

## [0.17.3] - 2022-02-05

### Bug Fixes

- Status report generation is prevented for bundles with source EID of dtn:none
- Buffering TCPCL pakets for efficiency
- Upgraded bp7 dependencies to make behaviour RFC9171 compliant

### Documentation

- Updated to point to new RFC 9171 and fixed some links
- Added comments to example configuration file

### Features

- Allow RUST_LOG to override debug level CLI parameteres, e.g., for enabling trace for different components
- Added error log with bundle hex string if decoding fails for further inspection

### Miscellaneous Tasks

- Upgraded to newest bp7 crate
- Made logging more fine-grained in ipnd, verbose parts now trace log level instead of debug

### Refactor

- Using clap derive interface for command line args, except for dtnd

### Testing

- Deactivated debug logging for midsize_fixed clab scenario

## [0.17.2] - 2022-02-01

### Bug Fixes

- Tcp refuse-existing-bundles behavior fixed, cla local/global options introduced (#10)
- Pinned tokio version to 1.15.0 to work around bug in 1.16.1
- Forced socket non-blocking for compability with new tokio releases

### Features

- Add bundle retransmission prevention extension to TCPCL (#8)
- (re)process oldest bundles first
- Added creation timestamp as well as received time to bundlepack meta data
- Peer address can be a generic broadcast, e.g., for use with LoRa

### Miscellaneous Tasks

- Removed dead code for transmission counters

### Refactor

- Changed websocket json data mode to encode byte buffers as base64
- Changed from tokio mutex to parking_lot for websockets
- Change project to workspace and include codegen crate (#11)

### Styling

- Fixed string related remarks found by clippy

### Testing

- Changed clab tests to use TCPCL instead of MTCPCL

## [0.17.1] - 2021-11-18

### Documentation

- Updated README and CLI help to reflect latest protocol and code changes
- Added `doc/http-client-api.md, documenting the http client api and websocket interface.
- Added section about JSON mode in http client API documentation
- Updated README to point to the different guides and include the new features

### Features

- Added `/json` mode for clients without CBOR and the `/node` command now returns the node id via ws

### Example

- Added `dtnecho_json.go` illustrating how to write clients using only JSON and websockets

## [0.17.0] - 2021-11-04

### Features

- Implemented TCP convergence layer draft (v28)

### Miscellaneous Tasks

- Upgraded d7sneakers version, it now bundles sqlite
- Added armv7 target for binary releases

### Refactor

- Changed DtnPeer to carry not only IpAddresses but a an enum with IpAddr and Generic(String)

## [0.16.16] - 2021-11-02

### Bug Fixes

- Fixed registering of non-singleton endpoints during startup of dtnd
- Removed misleading log message about "peer not seen" for static peers
- Made all test shell scripts normalize bundle counting output
- DtnPeer doctests would fail in some cases as static peers never timeout. Now only dynamic ones are generated for the test.
- Changed cbor decoding to also work on 32bit machines
- Upgraded to axum 0.3 to fix long compile times with rustc 1.56

### Styling

- Removed unneeded import in http cla

### Testing

- Added test for non-singleton group communication

## [0.16.15] - 2021-10-01

### Bug Fixes

- Eliminated potential deadlock in mtcp send_bundles

### Features

- Made http cla async

### Refactor

- Cleaned up logging of received bundles

### Testing

- Fixed typo in output of 3 node  scenario
- Added test chaining all CLAs (`cla_chain_test.sh`) with multiple nodes
- Added cla chain test to `run_all_tests.sh`
- Added mixsize-fixed clab scenario with 32 nodes and large bundle generation

## [0.16.14] - 2021-09-27

### Bug Fixes

- Only list bundle IDs for ones that have not been deleted

### Features

- Generic filter function for constraints

### Miscellaneous Tasks

- Removed obsolete TODO in process_bundles

### Refactor

- Made bundle removing explict processing::forward

### Testing

- Added test for status report generation to local_nodes_dtn.sh

## [0.16.13] - 2021-09-11

### Features

- Added config file and CLI options to enable/disable status report generation
- Websocket teardown now removes callback from application agents for subscribed endpoints

### Ci

- Added script to run all integration tests

## [0.16.12] - 2021-09-10

### Miscellaneous Tasks

- Cleaned up some leftover TODOs

### Build

- Updated bp7 to version with much less deps
- Added Cargo.lock

## [0.16.11] - 2021-09-09

### Features

- Added flag to output raw bundle instead of payload

<!-- generated by git-cliff -->
