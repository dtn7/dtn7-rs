Docker Testbed
==============

This directory contains various prebuilt network topologies using docker compose.
They can be run using the `run.sh` helper and should work on any machine with docker/podman compose installed.

You can then just use `docker exec` to work from any of the simulated nodes.

Optionally, use tools such as [pumba](https://github.com/alexei-led/pumba) to cause disruptions in the network connections.