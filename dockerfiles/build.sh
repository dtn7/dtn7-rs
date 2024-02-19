#/bin/bash

# if podman is installed use it
DOCKERCMD=docker
EXTRAS=""
if command -v podman &> /dev/null; then
  DOCKERCMD=podman
  EXTRAS="--format docker"
fi

$DOCKERCMD build -t gh0st42/dtn7:bookworm -f Dockerfile.debian-bookwork-slim $EXTRAS $@ ..

$DOCKERCMD build -t gh0st42/dtn7:alpine -f Dockerfile.alpine-slim $EXTRAS $@ ..