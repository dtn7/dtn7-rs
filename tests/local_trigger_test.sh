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

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

OUT_NODE1=$(mktemp /tmp/node1.XXXXXX)
PORT_NODE1=3000
#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE1 -C mtcp:port=2342 -e incoming -r epidemic -n node1 -s mtcp://127.0.0.1:4223/node2 $DB1 2>&1 &> $OUT_NODE1 &
PID_NODE1=$!
echo node1 pid: $PID_NODE1
echo node1 out: $OUT_NODE1
echo node1 port: $PORT_NODE1


OUT_NODE2=$(mktemp /tmp/node2.XXXXXX)
PORT_NODE2=3001
#DB2="-W /tmp/node2 -D sled"
#DB2="-W /tmp/node2 -D sneakers"
$DIR/../target/$TARGET/dtnd -d -j5s -i0 -w $PORT_NODE2 -C mtcp:port=4223 -e incoming -r epidemic \
    -n node2 \
    -s mtcp://127.0.0.1:2342/node1 \
    $DB2 2>&1 &> $OUT_NODE2 &
PID_NODE2=$!
echo node2 pid: $PID_NODE2
echo node2 out: $OUT_NODE2
echo node2 port: $PORT_NODE2

sleep 1

echo "Starting dtntrigger on node 2"
OUT_TRIGGER=$(mktemp /tmp/dtntrigger.XXXXXX)
$DIR/../target/$TARGET/dtntrigger -p $PORT_NODE2 -e incoming -c "echo INCOMING: " -v  2>&1 &> $OUT_TRIGGER &
PID_TRIGGER=$!
echo dtntrigger pid: $PID_TRIGGER
echo dtntrigger out: $OUT_TRIGGER

sleep 1

echo

echo "Sending 'test' to node 2"
echo test | $DIR/../target/$TARGET/dtnsend -r dtn://node2/incoming -p $PORT_NODE1

sleep 1 

echo "Sending 'test2' to node 2"
echo test2 | $DIR/../target/$TARGET/dtnsend -r dtn://node2/incoming -p $PORT_NODE1

sleep 2
echo

cat $OUT_TRIGGER

echo 

NUM_RECV=$(cat $OUT_TRIGGER | grep INCOMING | wc -l)
echo "Received on node 2: $NUM_RECV"
if [ $NUM_RECV -ne 2 ]; then
  RC=1
else 
  RC=0
fi
echo
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

kill $PID_NODE1 $PID_NODE2 $PID_TRIGGER
rm $OUT_NODE1 $OUT_NODE2 $OUT_TRIGGER

exit $RC