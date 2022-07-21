#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

PORT_NODE1=$(get_current_port)
start_dtnd -d -j5s -e incoming -r epidemic -n node1 --disable_nd -C mtcp:port=2342 -s mtcp://127.0.0.1:4223/node2

PORT_NODE2=$(get_current_port)
start_dtnd -d -j5s -e incoming -r epidemic \
  -n node2 \
  -C mtcp:port=4223 -s mtcp://127.0.0.1:2342/node1 \
  --interval 1s --ecla --disable_nd

PORT_NODE3=$(get_current_port)
start_dtnd -d -j5s -e incoming -r epidemic -n node3 --interval 1s --ecla --disable_nd

sleep 2

# Start ECLA Connector
start_task dtnecla_connect_n examples/dtnecla_connect_n -a 127.0.0.1:$PORT_NODE2 -a 127.0.0.1:$PORT_NODE3

sleep 1

echo

echo "Sending 'test' to node 3"
echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1

sleep 5

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

if [ -z "$STATUS_REPORTS" ]; then
  EXPECTED_BUNDLES=1
else
  EXPECTED_BUNDLES=2
fi
echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo
echo -n "Receiving on node 3: "
$BINS/dtnrecv -v -e incoming -p $PORT_NODE3
RC=$?
echo "RET: $RC"
echo

wait_for_key $1

cleanup

if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  exit $RC
else
  exit 1
fi
