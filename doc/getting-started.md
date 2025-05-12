# Getting started with dtn7

This document covers the basic usage of `dtn7` (daemon and some of the CLI tools) within the [core network emulator](https://github.com/coreemu/core). 
An older version of this guide is also available as a youtube video with instructions for macos:

[![Quickstart using dtn7 and coreemu on macOS](https://img.youtube.com/vi/7xwJEZyL_Ns/0.jpg)](https://www.youtube.com/watch?v=7xwJEZyL_Ns)

## Prerequisites

- [Install docker](https://docs.docker.com/get-docker/) - needed for testing in an emulated, virtual network
    * On Linux loading of `ebtables` and/or `sch_netem` kernel modules might be necessary (`sudo modprobe ebtables` / `sudo modprobe sch_netem`)
- For a quick start we are going to use the [dtn7-showroom](https://github.com/dtn7/dtn7-showroom). 


*Note: Most of the instructions apply to podman as well but depending on your host platform some parameters or security rules might be different.*

## Setting up the simulation environment

First, setup a shared directory and copy over necessary files:
```bash
mkdir /tmp/shared
cp tests/sink.sh /tmp/shared
```

The last file (`tests/sink.sh`) is only for demonstration purposes and not strictly necessary.

### Running the virtual showroom
```bash
docker run --rm -it                             \
    --name showroom                             \
    -p 5901:5901                                \
    -p 50052:50051                              \
    -p 2023:22                                  \
    -p 1190:1190                                \
    -p 6080:6080                                \
    -v /tmp/shared:/shared                      \
    --privileged                                \
    gh0st42/dtn7-showroom
```

### Connecting to the docker instance: CLI

Finally, we want to issue commands directly on the docker machine. Thus, we connect to it via:
```bash
docker exec -it showroom bash
```

We can then access all shared files within the docker instance in `/shared`. For easier access, it is recommended to copy all binaries to the `PATH` on the virtual machine.

### Connecting to the docker instance: GUI

A complete desktop with access to `core-gui` is provided through a web vnc on http://127.0.0.1:6080/vnc.html. The default session password is `sneakers`.

## Starting a simulation

### Configure wireless network

In the `core-gui` click on the network nodes symbol left of the canvas and select the *host* as node type. Then place it three times on the canvas.
Now select *wireless LAN* from the link node types and place it on the canvas.
Use the link tool (the one below the run and cursor buttons on the left side) and connect all nodes to the wifi.
By double-clicking each node you have to change the IPv4 subnet mask from `/32` to `/24`.

The simulation can be then started by pressing the green start button.
When dragging nodes in the running simulation around a green line should appear once they are in communication range.

By double-clicking a node in a running session you can get a terminal on this node.
To verify that everything works you should be able to `ping` nodes with a green line and when dragging them out of reach the packets should get lost.
If this is not the case you might be missing the proper kernel modules on your host system.

### Basic `dtnd` operation

By using the [core helper](https://github.com/gh0st42/core-helpers) scripts managing the simulation gets much easier.

To start `dtnd` on all running nodes enter the following in the bash that we previously connected to the docker instance:
```bash
cda 'dtnd -n $(hostname) -e incoming -C mtcp -r epidemic'
```

This starts `dtnd` with a node naming matching its hostname in the simulation, registers an endpoint called `incoming`, uses minimal tcp convergence layer and *epidemic* routing.
A full list of options can be seen by executing `dtnd -h`.

Each node has logs and a directory with local files under `/tmp/pycore.*/n<node number>.<conf/log>`.

The dtn daemon can be stopped and the standard out log can be cleared with the following command:
```bash
pkill -9 dtnd && cea rm nohup.out
```

*Note: dtn7-showroom has a preconfigured dtnd node type which you can also use instead of the above instructions.*

*Note2: The [core helpers](https://github.com/gh0st42/core-helpers) installed in the dtn7-showroom contain various little scripts to make life with core emu a bit easier.*

### Sample commands

#### Get global peer list

To get a list of all known peers from all nodes issue the following command in the bash session:
```bash
cea 'dtnquery peers | egrep "n[0-9].:" | cut -d \" -f 2 | sort'
```

#### Sending text message

Double click on the node you want to send a message from. This should open a terminal directly on this node.
To send a text message from node *n1* to node *n3* enter the following:
```bash
echo "hello world" | dtnsend -r dtn://n3/incoming
```


#### Receiving text message

Double click on the node you want to receive a message on. This should open a terminal directly on this node.
To receive a text message enter the following:
```bash
dtnrecv -e incoming
```

#### Reacting to incoming bundles

It is also possible to automatically trigger an external command for any incoming bundle to a specified endpoint. 
To execute the example script (`sink.sh`) enter the following command:
```bash
dtntrigger -e incoming -v -c /shared/sink.sh
```
