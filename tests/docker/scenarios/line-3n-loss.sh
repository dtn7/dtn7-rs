#!/bin/bash

NODES="line-3n-n1-1 line-3n-n2-1 line-3n-n3-1"

# Remember: define list of NODES before loading the helper functions
. $(dirname $0)/libnetemhelper.sh


while true; do
  loss_interval line-3n-n1-1 100% 30
  loss_interval line-3n-n3-1 100% 30
done
