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

OUT_NODE1=$(mktemp /tmp/node1.XXXXXX)
PORT_NODE1=3000
#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE1 -C tcp:port=2342 -e incoming -r epidemic -n node1 -s tcp://127.0.0.1:4223/node2 $DB1 $STATUS_REPORTS 2>&1 &> $OUT_NODE1 &
PID_NODE1=$!
echo node1 pid: $PID_NODE1
echo node1 out: $OUT_NODE1
echo node1 port: $PORT_NODE1


OUT_NODE2=$(mktemp /tmp/node2.XXXXXX)
PORT_NODE2=3001
#DB2="-W /tmp/node2 -D sled"
#DB2="-W /tmp/node2 -D sneakers"
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE2 -C tcp:port=4223 -e incoming -r epidemic \
    -n node2 \
    -s tcp://127.0.0.1:2342/node1 \
    -s tcp://127.0.0.1:2432/node3 \
    $DB2 2>&1 &> $OUT_NODE2 &
PID_NODE2=$!
echo node2 pid: $PID_NODE2
echo node2 out: $OUT_NODE2
echo node2 port: $PORT_NODE2

OUT_NODE3=$(mktemp /tmp/node3.XXXXXX)
PORT_NODE3=3002
#DB3="-W /tmp/node3 -D sled"
#DB3="-W /tmp/node3 -D sneakers"
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE3 -C tcp:port=2432 -e incoming -r epidemic -n node3 -s tcp://127.0.0.1:4223/node2 $DB3 $STATUS_REPORTS 2>&1 &> $OUT_NODE3 &
PID_NODE3=$!
echo node3 pid: $PID_NODE3
echo node3 out: $OUT_NODE3
echo node3 port: $PORT_NODE3

sleep 1

echo

echo "Sending 'test' to node 3"
echo test | $DIR/../target/$TARGET/dtnsend -r dtn://node3/incoming -p $PORT_NODE1

sleep 5

echo
echo -n "Bundles in store on node 1: "
NUM_BUNDLES=$($DIR/../target/$TARGET/dtnquery store | grep "dtn://" | wc -l | awk '{print $1}')
echo -n $NUM_BUNDLES

if [ -z "$STATUS_REPORTS" ]; then 
  EXPECTED_BUNDLES=1
else
  EXPECTED_BUNDLES=2
fi
echo " / $EXPECTED_BUNDLES"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]
then
    echo "Correct number of bundles in store!"
else
    echo "Incorrect number of bundles in store!"
fi
echo
echo -n "Receiving on node 3: "
$DIR/../target/$TARGET/dtnrecv -v -e incoming -p $PORT_NODE3
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

kill $PID_NODE1 $PID_NODE2 $PID_NODE3
rm $OUT_NODE1 $OUT_NODE2 $OUT_NODE3

if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ]; then
  exit $RC
else
  exit 1
fi