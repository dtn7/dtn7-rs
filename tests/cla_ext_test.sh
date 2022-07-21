#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

PORT_NODE1=3000
PORT_NODE2=3001
PORT_NODE3=3002
PORT_NODE4=3003
PORT_NODE5=3004

#refuse false
#expected no packet refusal
start_dtnd -d -j5s -i0 -C tcp:port=4224 -e incoming -r flooding -n node1 -s tcp://127.0.0.1:4225/node2
OUT_NODE1=$(echo $FILES | awk '{print $NF}')

#refuse true
#expected refuse packets
start_dtnd -d -j5s -i0 -O tcp:refuse-existing-bundles=true -C tcp:port=4225 -e incoming -r flooding -n node2 -s tcp://127.0.0.1:4226/node3
OUT_NODE2=$(echo $FILES | awk '{print $NF}')

#local refuse true, outgoing refuse false
#expected no packet refuses
#node 3 refuses packets from node 2 but node 4 does not refuse packets hence node 3 can not receive such refuse message
start_dtnd -d -j5s -i0 -O tcp:refuse-existing-bundles=false -C tcp:port=4226:refuse-existing-bundles=true -e incoming -r flooding -n node3 -s tcp://127.0.0.1:4227/node4
OUT_NODE3=$(echo $FILES | awk '{print $NF}')

#refuse false
#intermediate node needed before destination node
#expected no packet refuse
#local listener has refuse enabled but since
start_dtnd -d -j5s -i0 -O tcp:refuse-existing-bundles=false -C tcp:port=4227 -e incoming -r flooding -n node4 -s tcp://127.0.0.1:4228/node5
OUT_NODE4=$(echo $FILES | awk '{print $NF}')

#refuse false
#expected
start_dtnd -d -j5s -i0 -O tcp:refuse-existing-bundles=false -C tcp:port=4228 -e incoming -r flooding -n node5
OUT_NODE5=$(echo $FILES | awk '{print $NF}')

sleep 1

echo

echo "Sending 'test' to node 5"
echo test | $BINS/dtnsend -r dtn://node5/incoming -p $PORT_NODE1

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
echo -n "Receiving on node 5: "
$BINS/dtnrecv -v -e incoming -p $PORT_NODE5
RC=$?
echo "RET: $RC"
echo

wait_for_key $1

RF=0
echo "killing: $PIDS"
kill $PIDS
grep -i "Received refuse" $OUT_NODE2 -q
NODE2GREP=$?
echo "TCP retransmission check on node 2: expected: 0 found: $NODE2GREP"
grep -i "Received refuse" $OUT_NODE3 -q
NODE3GREP=$?
echo "TCP retransmission check on node 3: expected: 1 found: $NODE3GREP"
grep -i "Received refuse" $OUT_NODE1 -q
NODE1GREP=$?
echo "TCP retransmission check on node 1: expected: 1 found: $NODE1GREP"
grep -i "Received refuse" $OUT_NODE4 -q
NODE4GREP=$?
echo "TCP retransmission check on node 4: expected: 1 found: $NODE4GREP"
grep -i "Received refuse" $OUT_NODE5 -q
NODE5GREP=$?
echo "TCP retransmission check on node 5: expected: 1 found: $NODE5GREP"

if [ $NODE1GREP -eq 1 ] && [ $NODE4GREP -eq 1 ] && [ $NODE5GREP -eq 1 ] && [ $NODE3GREP -eq 1 ] && [ $NODE2GREP -eq 0 ]; then
  echo -n "TCP retransmission check: successful"
  echo
else
  echo -n "TCP retransmission check: failed"
  echo
  RF=1
fi

rm $FILES

if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  exit $RC || $RF
else
  exit 1
fi
