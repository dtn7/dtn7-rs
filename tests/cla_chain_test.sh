#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#STATUS_REPORTS="-g"

PORT_NODE1=$(get_current_port)
PORT_NODE2=$((PORT_NODE1 + 1))
PORT_NODE3=$((PORT_NODE1 + 2))
PORT_NODE4=$((PORT_NODE1 + 3))

#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
start_dtnd -d -j5s -i0 -C mtcp:port=2342 -e incoming -r epidemic \
  -n node1 -s mtcp://127.0.0.1:4223/node2 $DB1 $STATUS_REPORTS

#DB2="-W /tmp/node2 -D sled"
#DB2="-W /tmp/node2 -D sneakers"
start_dtnd -d -j5s -i0 -C mtcp:port=4223 -C http -e incoming \
  -r epidemic -n node2 \
  -s mtcp://127.0.0.1:2342/node1 \
  -s http://127.0.0.1:$PORT_NODE3/node3 $DB2

#DB3="-W /tmp/node3 -D sled"
#DB3="-W /tmp/node3 -D sneakers"
start_dtnd -d -j5s -i0 -C http -C tcp:port=4224 -e incoming \
  -r epidemic -n node3 \
  -s http://127.0.0.1:$PORT_NODE2/node2 \
  -s tcp://127.0.0.1:4225/node4 $STATUS_REPORTS

start_dtnd -d -j5s -i0 -C tcp:port=4225 -e incoming -r epidemic \
  -n node4 -s tcp://127.0.0.1:4224/node3 $DB4 $STATUS_REPORTS

sleep 1

echo

echo "Sending 'test' to node 4"
echo test | $BINS/dtnsend -r dtn://node4/incoming -p $PORT_NODE1

sleep 5

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo $NUM_BUNDLES

if [ -z "$STATUS_REPORTS" ]; then
  EXPECTED_BUNDLES=1
else
  EXPECTED_BUNDLES=2
fi

if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo
echo -n "Receiving on node 4: "
$BINS/dtnrecv -v -e incoming -p $PORT_NODE4
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
