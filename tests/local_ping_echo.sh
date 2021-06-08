#!/bin/bash

cargo build --bins --examples

if [ $? -ne 0 ]
then
  echo "Build failed."
  exit 1
fi

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

OUT_NODE1=$(mktemp /tmp/node1.XXXXXX)
PORT_NODE1=3000
#DB1="-W /tmp/node1 -D sled"
$DIR/../target/debug/dtnd -d -w $PORT_NODE1 -C mtcp:2342 -e incoming -r epidemic -n node1 $DB1 2>&1 &> $OUT_NODE1 &
PID_NODE1=$!
echo node1 pid: $PID_NODE1
echo node1 out: $OUT_NODE1
echo node1 port: $PORT_NODE1

sleep 1

OUT_ECHO1=$(mktemp /tmp/echo1.XXXXXX)
$DIR/../target/debug/examples/dtnecho2 2>&1 &> $OUT_ECHO1 &
PID_ECHO1=$!
echo echo1 pid: $PID_ECHO1
echo echo1 out: $OUT_ECHO1

echo

echo "Sending 3 pings to node1"
$DIR/../target/debug/examples/dtnping -t 'dtn://node1/echo' -c 3

echo "Press any key to stop daemons and clean up logs"
read -n 1
kill $PID_NODE1 $PID_NODE2 $PID_NODE3
rm $OUT_NODE1 $OUT_NODE2 $OUT_NODE3
