# automated network tests

This directory contains example scenarios that use [coreemu-lab](https://github.com/gh0st42/coreemu-lab) to automatically run dtn7 on a number of mobile nodes and gather various statistics from the simulation.

This requires docker to be present and precompiled binaries for dtn7-rs.
At the moment the scenario runner only support linux, macOS support is planned in the future.

## quick start

A few tools are provided in this folder:

- `clab` is the standard shell starter provided by *coreemu-lab*
- `run_scenario` can be used to start a headless instance of any of the provided scenario subfolders
- `clean_scenarios` clears all results and intermediate files from all scenario subdirectories

## example scenarios

- `scenario_basic/` - 5 nodes with a random walk mobility pattern, just doing ipnd neighborhood discovery, no further bundles are generated or exchanged.

- `scenario_msgs/` - 5 nodes with a random walk mobility pattern, txt bundles are randomly generated and sent to any of the 5 nodes.

- `scenario_3n_delayed/` - 3 nodes starting disconnected, n2 appears near n1, then moves out of range and appears near n3, delivering its messages. Messages sent logged on n1 in `sent.log` and received ones are in `recv.log` on n3. Scenario shows the usage of `dtnsend` and `dtntrigger`.