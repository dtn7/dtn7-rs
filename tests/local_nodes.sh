#!/bin/bash

cargo build

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"

OUT_NODE1=$(mktemp /tmp/node1.XXXXXX)
PORT_NODE1=3000
$DIR/../target/debug/dtnd -d -j0 -i0 -w $PORT_NODE1 -C mtcp:2342 -e incoming -r epidemic -n node1 -s mtcp://127.0.0.1:4223/node2 2>&1 &> $OUT_NODE1 &
PID_NODE1=$!
echo node1 pid: $PID_NODE1
echo node1 out: $OUT_NODE1
echo node1 port: $PORT_NODE1


OUT_NODE2=$(mktemp /tmp/node2.XXXXXX)
PORT_NODE2=3001
$DIR/../target/debug/dtnd -d -j0 -i0 -w $PORT_NODE2 -C mtcp:4223 -e incoming -r epidemic -n node2 -s mtcp://127.0.0.1:2342/node1 2>&1 &> $OUT_NODE2 &
PID_NODE2=$!
echo node2 pid: $PID_NODE2
echo node2 out: $OUT_NODE2
echo node2 port: $PORT_NODE2

echo "Press any key to stop daemons and clean up logs"
read -n 1
kill $PID_NODE1 $PID_NODE2
rm $OUT_NODE1 $OUT_NODE2