#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

PORT_NODE1=$(get_current_port)
start_dtnd -d -j5s -i0 -C mtcp:port=2342 -e incoming -r epidemic -n node1 -s mtcp://127.0.0.1:4223/node2 $STATUS_REPORTS

PORT_NODE2=$(get_current_port)
start_dtnd -d -j5s -i0 -C mtcp:port=4223 -e incoming -r external \
  -n node2 \
  -s mtcp://127.0.0.1:2342/node1 \
  -s mtcp://127.0.0.1:2432/node3

PORT_NODE3=$(get_current_port)
start_dtnd -d -j5s -i0 -e incoming -r epidemic -n node3 -s mtcp://127.0.0.1:4223/node2 --ecla $STATUS_REPORTS

# Start ECLA
sleep 1

start_task dtnecla_mtcp examples/dtnecla_mtcp -a 127.0.0.1:$PORT_NODE3 -p 2432

start_task dtnerouting examples/dtnerouting -a 127.0.0.1:$PORT_NODE2 -t epidemic

sleep 2

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
