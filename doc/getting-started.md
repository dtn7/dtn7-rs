# Getting started with dtn7

This document covers the basic usage of `dtn7` (daemon and some of the CLI tools) within the [core network emulator](https://github.com/coreemu/core). This content is also available as a youtube video with instructions for macos:

[![Quickstart using dtn7 and coreemu on macOS](https://img.youtube.com/vi/7xwJEZyL_Ns/0.jpg)](https://www.youtube.com/watch?v=7xwJEZyL_Ns)

## Prerequisites

- [Install rust](https://www.rust-lang.org/tools/install) - needed to compile `dtn7`
- [Install docker](https://docs.docker.com/get-docker/) - needed for testing in an emulated, virtual network
    * On Linux loading of `ebtables` kernel module might be necessary (`sudo modprobe ebtables`)
- On non-x64 Linux platforms, `cross` is helpful to cross-compile for the docker container (`cargo install cross`)
- On non-linux platforms, a x-server is needed (e.g., [xquartz](https://www.xquartz.org/) on macOS)

## Building 

First, you need to checkout the most recent version of the `dtn7` source:
```bash
git clone https://github.com/dtn7-rs
cd dtn7-rs
```

Depending on the platform you can then build the various binaries of `dtn7`, optionally append the `--release` flag.

For x64 linux use `cargo` directly:

```bash
cargo build
```

For other platforms cross-compile it using the following command:

```bash
cross build --target=x86_64-unknown-linux-gnu
```

## Setting up the simulation environment

First, setup a shared directory and copy over necessary files:
```bash
mkdir /tmp/shared
cp target/x86_64-unknown-linux-gnu/debug/dtn* /tmp/shared/
cp tests/sink.sh /tmp/shared
```

The last file (`tests/sink.sh`) is only for demonstration purposes and not strictly necessary.

### Docker on Linux
```bash
sudo modprobe ebtables
xhost +local:root
docker run --rm \
    --name coreemu \
    -p 2000:22 \
    -v /tmp/shared:/shared \
    --cap-add=NET_ADMIN \
    --cap-add=SYS_ADMIN \
    -e SSHKEY="`cat ~/.ssh/id_rsa.pub`" \
    -e DISPLAY \
    -v /tmp/.X11-unix:/tmp/.X11-unix:rw \
    --privileged \
    -d \
    gh0st42/coreemu7
```


### Docker on macOS
```bash
xhost + 127.0.0.1
docker run -it --rm \
    --name coreemu6 \
    -p 2000:22 \
    -v /tmp/shared:/shared \
    --cap-add=NET_ADMIN \
    --cap-add=SYS_ADMIN \
    -e SSHKEY="`cat ~/.ssh/id_rsa.pub`" \
    -e DISPLAY=host.docker.internal:0 \
    --privileged \
    -d \
    gh0st42/coreemu7
```

### Connecting to docker instance

Finally, we want to issue commands directly on the docker machine. Thus, we connect via ssh (without remembering the temporary ssh server fingerprint):
```bash
alias nossh='ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null'

nossh -p 2000 root@localhost
```

If no ssh key is present then the default password for the docker container is `netsim`.

We can then access all shared files within the docker instance in `/shared`. For easier access, it is recommended to copy all binaries to the `PATH` on the virtual machine.
```bash
cp /shared/dtn* /usr/local/bin
```

## Starting a simulation

### Configure wireless network

In the `core-gui` click on the network nodes symbol left of the canvas and select the *host* as node type. Then place it three times on the canvas.
Now select *wireless LAN* from the link node types and place it on the canvas.
By double-clicking it you can change the wifi properties. Change the IPv4 subnet mask from `/32` to `/24`, press *link to all routers*, and finally click *apply*.

The simulation can be then started by pressing the green start button.
When dragging nodes in the running simulation around a green line should appear once they are in communication range.

### Basic `dtnd` operation

By using the [core helper](https://github.com/gh0st42/core-helpers) scripts managing the simulation gets much easier.

To start `dtnd` on all running nodes enter the following in the ssh session connected to the docker instance:
```bash
cda 'dtnd -n $(hostname) -e incoming -C mtcp -r epidemic'
```

This starts `dtnd` with a node naming matching its hostname in the simulation, registers an endpoint called `incoming`, uses minimal tcp convergence layer and *epidemic* routing.
A full list of options can be seen by executing `dtnd -h`.

Each node has a directly with logs and local files under `/tmp/pycore.*/n<node number>`.

The dtn daemon can be stopped and the standard out log can be cleared with the following command:
```bash
pkill -9 dtnd && cea rm nohup.out
```

### Sample commands

#### Get global peer list

To get a list of all known peers from all nodes issue the following command in the ssh session:
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
