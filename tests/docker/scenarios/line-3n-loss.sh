#!/bin/bash

DOCKERCMD=docker
# use podman if available
if command -v podman &> /dev/null; then
  DOCKERCMD=podman
fi
$DOCKERCMD exec line-3n-n1-1 tc qdisc add dev eth0 root netem loss 0%
$DOCKERCMD exec line-3n-n2-1 tc qdisc add dev eth0 root netem loss 0%
$DOCKERCMD exec line-3n-n3-1 tc qdisc add dev eth0 root netem loss 0%

cleanup () {
  echo "Cleaning up..."
  $DOCKERCMD exec line-3n-n1-1 tc qdisc del dev eth0 root netem loss 0%
  $DOCKERCMD exec line-3n-n2-1 tc qdisc del dev eth0 root netem loss 0%
  $DOCKERCMD exec line-3n-n3-1 tc qdisc del dev eth0 root netem loss 0%
}

trap cleanup EXIT


while true; do
  echo "Setting loss on n3 to 100% for 30s" 
  $DOCKERCMD exec line-3n-n3-1 tc qdisc change dev eth0 root netem loss 100%
  sleep 30
  echo " n3 back to 0% loss." 
  $DOCKERCMD exec line-3n-n3-1 tc qdisc change dev eth0 root netem loss 0%
  echo "Setting loss on n1 to 100% for 30s"	
  $DOCKERCMD exec line-3n-n1-1 tc qdisc change dev eth0 root netem loss 100%
  sleep 30
  echo " n1 back to 0% loss."
  $DOCKERCMD exec line-3n-n1-1 tc qdisc change dev eth0 root netem loss 0%
done
