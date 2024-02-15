#!/bin/bash

cleanup () {
  echo "Cleaning up..."
  docker compose -f $COMPOSE_FILE kill
  docker compose -f $COMPOSE_FILE down -v --remove-orphans
}

# print help if no arguments are given
if [ $# -eq 0 ]; then
  echo "Usage: $0 <scenario>"
  exit 1
fi

COMPOSE_FILE=$1

echo "Running scenario $COMPOSE_FILE"

trap cleanup EXIT

docker compose -f $COMPOSE_FILE up --force-recreate --build --remove-orphans -d

docker compose -f $COMPOSE_FILE logs -f
