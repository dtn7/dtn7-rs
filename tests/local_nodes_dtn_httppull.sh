#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#STATUS_REPORTS="-g"

#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
PORT_NODE1=$(get_current_port)
start_dtnd -d -j2s -i0 -C httppull -e incoming -r sink -n node1 -s tcp://127.0.0.1:$PORT_NODE2/node2 $DB1 $STATUS_REPORTS

PORT_NODE2=$(get_current_port)
#DB2="-W /tmp/node2 -D sled"
#DB2="-W /tmp/node2 -D sneakers"
start_dtnd -d -j2s -i0 -C httppull -e incoming -r sink -n node2 -s tcp://127.0.0.1:$PORT_NODE1/node1 -s tcp://127.0.0.1:$PORT_NODE3/node3 $DB2

PORT_NODE3=$(get_current_port)
#DB3="-W /tmp/node3 -D sled"
#DB3="-W /tmp/node3 -D sneakers"
start_dtnd -d -j2s -i0 -C httppull -e incoming -r sink -n node3 -s tcp://127.0.0.1:$PORT_NODE2/node2 $DB3 $STATUS_REPORTS

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
