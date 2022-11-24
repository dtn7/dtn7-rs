#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

PORT_NODE1=$(get_current_port)
start_dtnd -d -j2s -i0 -C tcp:port=2342 -e incoming -r epidemic -n node1

sleep 1

echo

echo "Sending 'test' to node 3 with a lifetime of 2 seconds"
echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1 -l 2
echo "Sending 'test' to self with a lifetime of 2 seconds"
echo test | $BINS/dtnsend -r dtn://node1/incoming -p $PORT_NODE1 -l 2

#$BINS/dtnquery store
echo
echo "Waiting for 5 seconds"
sleep 5

#$BINS/dtnquery store

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery bundles | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=0

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi

wait_for_key $1

cleanup

if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  exit 0
else
  exit 1
fi
