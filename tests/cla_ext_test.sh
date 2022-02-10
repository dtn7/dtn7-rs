#!/bin/bash

TARGET=release
#TARGET=debug

if [ $TARGET = "debug" ]; then
  cargo build --bins
else 
  cargo build --release --bins
fi

if [ $? -ne 0 ]
then
  echo "Build failed."
  exit 1
fi

#STATUS_REPORTS="-g"

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"


PORT_NODE1=3000
PORT_NODE2=3001
PORT_NODE3=3002
PORT_NODE4=3003
PORT_NODE5=3004

#refuse false
#expected no packet refusal
OUT_NODE1=$(mktemp /tmp/node1.XXXXXX)
#DB3="-W /tmp/node3 -D sled"
#DB3="-W /tmp/node3 -D sneakers"
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE1 -C tcp:port=4224 -e incoming -r flooding -n node1 -s tcp://127.0.0.1:4225/node2 $STATUS_REPORTS 2>&1 &> $OUT_NODE1 &
PID_NODE1=$!
echo node1 pid: $PID_NODE1
echo node1 out: $OUT_NODE1
echo node1 port: $PORT_NODE1

#refuse true
#expected refuse packets
OUT_NODE2=$(mktemp /tmp/node2.XXXXXX)
#DB3="-W /tmp/node3 -D sled"
#DB3="-W /tmp/node3 -D sneakers"
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE2 -O tcp:refuse-existing-bundles=true -C tcp:port=4225 -e incoming -r flooding -n node2 -s tcp://127.0.0.1:4226/node3 $STATUS_REPORTS 2>&1 &> $OUT_NODE2 &
PID_NODE2=$!
echo node2 pid: $PID_NODE2
echo node2 out: $OUT_NODE2
echo node2 port: $PORT_NODE2

#local refuse true, outgoing refuse false
#expected no packet refuses
#node 3 refuses packets from node 2 but node 4 does not refuse packets hence node 3 can not receive such refuse message
OUT_NODE3=$(mktemp /tmp/node3.XXXXXX)
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE3 -O tcp:refuse-existing-bundles=false -C tcp:port=4226:refuse-existing-bundles=true -e incoming -r flooding -n node3 -s tcp://127.0.0.1:4227/node4 $DB4 $STATUS_REPORTS 2>&1 &> $OUT_NODE3 &
PID_NODE3=$!
echo node3 pid: $PID_NODE3
echo node3 out: $OUT_NODE3
echo node3 port: $PORT_NODE3

#refuse false
#intermediate node needed before destination node
#expected no packet refuse
#local listener has refuse enabled but since 
OUT_NODE4=$(mktemp /tmp/node4.XXXXXX)
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE4 -O tcp:refuse-existing-bundles=false -C tcp:port=4227 -e incoming -r flooding -n node4  -s tcp://127.0.0.1:4228/node5 $DB4 $STATUS_REPORTS 2>&1 &> $OUT_NODE4 &
PID_NODE4=$!
echo node4 pid: $PID_NODE4
echo node4 out: $OUT_NODE4
echo node4 port: $PORT_NODE4

#refuse false
#expected
OUT_NODE5=$(mktemp /tmp/node5.XXXXXX)
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE5 -O tcp:refuse-existing-bundles=false -C tcp:port=4228 -e incoming -r flooding -n node5 $DB4 $STATUS_REPORTS 2>&1 &> $OUT_NODE5 &
PID_NODE5=$!
echo node5 pid: $PID_NODE5
echo node5 out: $OUT_NODE5
echo node5 port: $PORT_NODE5

sleep 1

echo

echo "Sending 'test' to node 5"
echo test | $DIR/../target/$TARGET/dtnsend -r dtn://node5/incoming -p $PORT_NODE1

sleep 5

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($DIR/../target/$TARGET/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo $NUM_BUNDLES

if [ -z "$STATUS_REPORTS" ]; then 
  EXPECTED_BUNDLES=1
else
  EXPECTED_BUNDLES=2
fi

if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]
then
    echo "Correct number of bundles in store!"
else
    echo "Incorrect number of bundles in store!"
fi
echo
echo -n "Receiving on node 5: "
$DIR/../target/$TARGET/dtnrecv -v -e incoming -p $PORT_NODE5
RC=$? 
echo "RET: $RC" 
echo 

if [[ $1 = "-k" ]]; then
  echo "Press any key to stop daemons and clean up logs"
  read -n 1
else
  echo
  echo "Provide -k as parameter to keep session running."
  echo
fi

RF=0

kill $PID_NODE1 $PID_NODE2 $PID_NODE3 $PID_NODE4 $PID_NODE5
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

if [ $NODE1GREP -eq 1 ] && [ $NODE4GREP -eq 1 ] && [ $NODE5GREP -eq 1 ] && [ $NODE3GREP -eq 1 ] && [ $NODE2GREP -eq 0 ]
then
  echo -n "TCP retransmission check: successful"
  echo
else
  echo -n "TCP retransmission check: failed"
  echo
  RF=1
fi

rm $OUT_NODE1 $OUT_NODE2 $OUT_NODE3 $OUT_NODE4 $OUT_NODE5

if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  exit $RC || $RF
else
  exit 1
fi