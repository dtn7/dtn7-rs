#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

STORE_MAX_SIZE=120
echo "Store max size: $STORE_MAX_SIZE bytes"

#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
PORT_NODE1=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=2342 -e incoming -r epidemic --max-store-size 120 -n node1 $DB1 

sleep 1

echo

echo "Sending message #1 to node 3"
echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1 | grep "Result:"

sleep 1

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
echo
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo
echo "Sending message #2 to node 3"
echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1 | grep "Result:"

sleep 1

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
echo
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
