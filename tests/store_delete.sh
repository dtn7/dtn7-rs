#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
PORT_NODE1=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=2342 -e incoming -r epidemic -n node1 $DB1 

sleep 1

echo

echo "Sending 'test' to node 3"
BID=$(echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1 | grep "dtn://" | awk '{print $2}')

sleep 1

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo
echo "Deleting bundle $BID on node 1: "
$BINS/dtnrecv -v --delete $BID -p $PORT_NODE1
#curl http://127.0.0.1/1/store/$BID -X DELETE
#curl "http://127.0.0.1:$PORT_NODE1/delete?$BID"
RC=$?
echo
echo "RET: $RC"
echo
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
  exit $RC
else
  exit 1
fi
