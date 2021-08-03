#!/bin/bash

TARGET=release
#TARGET=debug

if [ $TARGET = "debug" ]; then
  cargo build --bins --examples
else 
  cargo build --release --bins --examples
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
$DIR/../target/$TARGET/dtnd -d -i0 -w $PORT_NODE1 -C mtcp:2342 -r epidemic -n node1 $DB1 2>&1 &> $OUT_NODE1 &
PID_NODE1=$!
echo node1 pid: $PID_NODE1
echo node1 out: $OUT_NODE1
echo node1 port: $PORT_NODE1

sleep 1

OUT_ECHO1=$(mktemp /tmp/echo1.XXXXXX)
$DIR/../target/$TARGET/examples/dtnecho2 -v 2>&1 &> $OUT_ECHO1 &
PID_ECHO1=$!
echo echo1 pid: $PID_ECHO1
echo echo1 out: $OUT_ECHO1

echo

echo "Sending 3 pings to node1"
$DIR/../target/$TARGET/examples/dtnping -t 'dtn://node1/echo' -c 6

echo "Press any key to stop daemons and clean up logs"
read -n 1
kill $PID_NODE1 $PID_ECHO1
rm $OUT_NODE1 $OUT_ECHO1