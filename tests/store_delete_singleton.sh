#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
PORT_NODE1=$(get_current_port)
start_dtnd -d -j5s -i0 -C tcp:port=2342 -e incoming -e ~group -r epidemic -n node1 $DB1

sleep 0.5

echo

echo "Sending 'test' to node 1"
BID_SINGLE=$(echo test | $BINS/dtnsend -r dtn://node1/incoming -p $PORT_NODE1 | grep "dtn://" | awk '{print $2}')
BID_GRP=$(echo test | $BINS/dtnsend -r dtn://node1/~group -p $PORT_NODE1 | grep "dtn://" | awk '{print $2}')

sleep 0.5

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=2

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"

fi
echo

# Receive bundle IDs - should not change number of bundles
echo "Receiving bundles $BID_SINGLE and $BID_GRP on node 1: "
$BINS/dtnrecv -v -b $BID_SINGLE -p $PORT_NODE1 || exit $?
$BINS/dtnrecv -v -b $BID_GRP -p $PORT_NODE1 || exit $?
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery bundles | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=2

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
  wait_for_key $1
  cleanup
  exit 1
fi


# Receive singleton endpoint - should decrease number of bundles to 1
echo "Receiving from endpoint 'incoming' on node 1: "
$BINS/dtnrecv -v -e incoming -p $PORT_NODE1
RC=$?
echo
echo "RET: $RC"
echo
echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery bundles | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
  wait_for_key $1
  cleanup
  exit 1
fi

# Receive group endpoint - should not change number of bundles
echo "Receiving from endpoint '~group' on node 1: "
$BINS/dtnrecv -v -e ~group -p $PORT_NODE1
RC=$?
echo
echo "RET: $RC"
echo
echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($BINS/dtnquery bundles | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

EXPECTED_BUNDLES=1

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
