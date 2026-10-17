Docker Testbed
==============

The `scenarios/` directory contains various prebuilt network topologies using docker compose.
They can be run using the `run.sh` helper and should work on any machine with docker/podman compose installed.

You can then just use `docker exec` to work from any of the simulated nodes.

Optionally, use tools such as [pumba](https://github.com/alexei-led/pumba) to cause disruptions in the network connections.

An example on how to implement periodically alternating connectivity can also be found in the `scenarios/line-3n-loss.sh` script, which should be run after starting the `line-3n.yml` scenario.

The `bpsec-3n.yml` scenario demonstrates [BPSec](../../doc/bpsec.md) security rules and is built from the local sources.
After starting it, `scenarios/bpsec-3n-check.sh` sends bundles through it and checks that they are protected as configured.

To build complex network topologies, tools such as [netedit](https://github.com/gh0st42/PONS/tree/master/tools/netedit) can be used to generate a graphml file of the network.
For convenience, a helper script, `graphml2docker.py` is provided, which automatically creates a docker compose file for the given topology.
