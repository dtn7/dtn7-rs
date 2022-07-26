#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

PORT_NODE1=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=2342 -e incoming -r sprayandwait -n node1 -s tcp://127.0.0.1:4223/node2 -s tcp://127.0.0.1:4224/node4 -s tcp://127.0.0.1:4225/node5

PORT_NODE2=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=4223 -e incoming -r sprayandwait -n node2 -s tcp://127.0.0.1:2342/node1 -s tcp://127.0.0.1:2432/node3

PORT_NODE3=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=2432 -e incoming -r sprayandwait -n node3 -s tcp://127.0.0.1:4223/node2

PORT_NODE4=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=4224 -e incoming -r sprayandwait -n node4 -s tcp://127.0.0.1:2342/node1

PORT_NODE5=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=4225 -e incoming -r sprayandwait -n node5 -s tcp://127.0.0.1:2342/node1

sleep 1

echo

echo "Sending 'test' to node 3"
echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1

sleep 5
TOTAL_BUNDLES=0

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

# increase the total bundles by the number of bundles on node 1
TOTAL_BUNDLES=$(($TOTAL_BUNDLES + $NUM_BUNDLES))

EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo

echo -n "Bundles in store on node 2: "
NUM_BUNDLES=$($BINS/dtnquery -p $PORT_NODE2 store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

TOTAL_BUNDLES=$(($TOTAL_BUNDLES + $NUM_BUNDLES))
EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo

echo -n "Bundles in store on node 3: "
NUM_BUNDLES=$($BINS/dtnquery -p $PORT_NODE3 store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

TOTAL_BUNDLES=$(($TOTAL_BUNDLES + $NUM_BUNDLES))
EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo

echo -n "Bundles in store on node 4: "
NUM_BUNDLES=$($BINS/dtnquery -p $PORT_NODE4 store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

TOTAL_BUNDLES=$(($TOTAL_BUNDLES + $NUM_BUNDLES))
EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo

echo -n "Bundles in store on node 5: "
NUM_BUNDLES=$($BINS/dtnquery -p $PORT_NODE5 store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

TOTAL_BUNDLES=$(($TOTAL_BUNDLES + $NUM_BUNDLES))
EXPECTED_BUNDLES=0

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

if [ "$TOTAL_BUNDLES" = "4" ]; then
  exit $RC
else
  exit 1
fi
