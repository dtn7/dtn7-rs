#!/bin/bash

DOCKERCMD=docker
# use podman if available
if command -v podman &> /dev/null; then
  DOCKERCMD=podman
fi

cleanup () {
  echo "Cleaning up..."
  $DOCKERCMD compose -f $COMPOSE_FILE kill
  $DOCKERCMD compose -f $COMPOSE_FILE down -v --remove-orphans
}

# print help if no arguments are given
if [ $# -eq 0 ]; then
  echo "Usage: $0 <scenario>"
  exit 1
fi

COMPOSE_FILE=$1

echo "Running scenario $COMPOSE_FILE"

trap cleanup EXIT

$DOCKERCMD compose -f $COMPOSE_FILE up --force-recreate --build --remove-orphans -d

$DOCKERCMD compose -f $COMPOSE_FILE logs -f
