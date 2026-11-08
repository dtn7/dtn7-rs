#!/bin/bash

# PRoPHET v1 routing test on a 3 node line topology: node1 <-> node2 <-> node3
# Uses static peers without discovery, relying on the agent's internal contact tracking.
# Encounters are staged:
# node2 and node3 meet first and exchange predictability maps,
# node1 joins later and learns about node3 from node2's reply to its greeting.
# node2's initial map towards the still-absent node1 fails and is discarded,
# so only the later reply path can inform node1.
# A bundle from node1 must then reach node3 via node2.

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

PROPHET_OPTS="-r prophet_v1 -R prophet_v1.tick_interval=1s"

PORT_NODE2=$(get_current_port)
start_dtnd -d -j2s -i0 -C tcp:port=4223 -e incoming $PROPHET_OPTS -n node2 -s tcp://127.0.0.1:2342/node1 -s tcp://127.0.0.1:2432/node3

PORT_NODE3=$(get_current_port)
start_dtnd -d -j2s -i0 -C tcp:port=2432 -e incoming $PROPHET_OPTS -n node3 -s tcp://127.0.0.1:4223/node2

echo
echo "Waiting for node2 and node3 to exchange predictability maps"
sleep 8

PORT_NODE1=$(get_current_port)
start_dtnd -d -j2s -i0 -C tcp:port=2342 -e incoming $PROPHET_OPTS -n node1 -s tcp://127.0.0.1:4223/node2

echo
echo "Waiting for node1 to greet node2 and receive its reply"
sleep 6

echo -n "node1 predictabilities: "
GETDATA=$(curl -s "http://127.0.0.1:$PORT_NODE1/routing/getdata")
echo $GETDATA
if echo $GETDATA | grep -q '"node3":0.140625'; then
  echo "node1 knows node3 from exactly one transitive update!"
  GETDATA_OK=0
else
  echo "node1 has no valid predictability for node3!"
  GETDATA_OK=1
fi
echo

echo "Sending 'test' to node3"
echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1

sleep 8
TOTAL_BUNDLES=0

echo
echo -n "Bundles in store on node1: "
NUM_BUNDLES=$($BINS/dtnquery -p $PORT_NODE1 store | grep "dtn://" | grep -v Deleted | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

TOTAL_BUNDLES=$(($TOTAL_BUNDLES + $NUM_BUNDLES))

# node1 keeps its copy, no metadata bundles may linger
EXPECTED_BUNDLES=1

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo

echo -n "Bundles in store on node2: "
NUM_BUNDLES=$($BINS/dtnquery -p $PORT_NODE2 store | grep "dtn://" | grep -v Deleted | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

TOTAL_BUNDLES=$(($TOTAL_BUNDLES + $NUM_BUNDLES))

# node2 delivered directly to node3 and deleted its copy afterwards
EXPECTED_BUNDLES=0

echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
fi
echo

echo -n "Bundles in store on node3: "
NUM_BUNDLES=$($BINS/dtnquery -p $PORT_NODE3 store | grep "dtn://" | grep -v Deleted | wc -l | awk '{print $1}')
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

echo -n "Receiving on node3: "
$BINS/dtnrecv -v -e incoming -p $PORT_NODE3
RC=$?
echo "RET: $RC"
echo

wait_for_key $1

cleanup

if [ "$TOTAL_BUNDLES" = "2" ] && [ "$GETDATA_OK" = "0" ]; then
  exit $RC
else
  exit 1
fi
