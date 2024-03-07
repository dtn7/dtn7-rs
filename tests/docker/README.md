Docker Testbed
==============

The `scenarios/` directory contains various prebuilt network topologies using docker compose.
They can be run using the `run.sh` helper and should work on any machine with docker/podman compose installed.

You can then just use `docker exec` to work from any of the simulated nodes.

Optionally, use tools such as [pumba](https://github.com/alexei-led/pumba) to cause disruptions in the network connections.

An example on how to implement periodically alternating connectivity can be found in the `scenarios/line-3n-loss.sh` script, which should be run after starting the `line-3n.yml` scenario.
