#!/bin/bash

get_peers() {
  local PEERS=$(curl -s -f --show-error "http://127.0.0.1:$1/status/peers") #| jq -r '.peers[].peer'
  if [ $? -ne 0 ]; then
    echo "FATAL: Failed to call rest function!"
    cleanup
    exit 1
  fi
  if [ "$PEERS" = "{}" ]; then
    echo $PEERS
  else
    #echo $PEERS | jq -r '.peers[].peer'
    echo $PEERS | jq 'keys' | awk '{print $1}' | grep '"'
  fi
}

add_peer() {
  echo "> Adding new peer $2 of type $3"
  curl -s -f --show-error "http://127.0.0.1:$1/peers/add?p=$2&p_t=$3"
  if [ $? -ne 0 ]; then
    echo "FATAL: Failed to call rest function!"
    cleanup
    exit 1
  fi
}

del_peer() {
  echo "> Deleting peer $2"
  curl -s -f --show-error "http://127.0.0.1:$1/peers/del?p=$2"
  if [ $? -ne 0 ]; then
    echo "FATAL: Failed to call rest function!"
    sleep 60
    cleanup
    exit 1
  fi
}

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#STATUS_REPORTS="-g"


PORT_NODE1=$(get_current_port)
PEER2=tcp://127.0.0.1:4223/node2
start_dtnd -d -j5s -i0 -C tcp:port=2342 -e incoming -r epidemic -n node1

PORT_NODE2=$(get_current_port)
PEER1=tcp://127.0.0.1:2342/node1
start_dtnd -d -j5s -i0 -C tcp:port=4223 -e incoming -r epidemic -n node2

sleep 1

PEERS=$(get_peers $PORT_NODE1)

if [ "$PEERS" = "{}" ]; then
  echo "Peers on node 1: $PEERS"
else
  echo "FATAL: Incorrect peers on node 1!"
  cleanup
  exit 1
fi

add_peer $PORT_NODE1 $PEER2 "DYNAMIC"
echo 

PEERS=$(get_peers $PORT_NODE1 | wc -l)
if [ "$PEERS" -eq 1 ]; then
 echo "Number of peers on node 1: $PEERS"
else
  echo "FATAL: Incorrect peers on node 1: ${PEERS}"
  cleanup
  exit 1
fi
echo

echo "Sending 'test' to node 2"
echo test | $BINS/dtnsend -r dtn://node2/incoming -p $PORT_NODE1

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
  cleanup
  exit 1
fi
echo
echo -n "Receiving on node 2: "
$BINS/dtnrecv -v -e incoming -p $PORT_NODE2
if [ $? -ne 0 ]; then
  echo "FATAL: Failed to receive!"
  cleanup
  exit 1
fi
echo

del_peer $PORT_NODE1 $PEER2
echo 

PEERS=$(get_peers $PORT_NODE1)
if [ "$PEERS" = "{}" ]; then
 echo "Number of peers on node 1: $PEERS"
else
  echo "FATAL: Incorrect peers on node 1: ${PEERS}"
  cleanup
  exit 1
fi
echo

wait_for_key $1

cleanup

exit 0 
