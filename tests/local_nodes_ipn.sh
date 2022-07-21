#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#STATUS_REPORTS="-g"

PORT_NODE1=$(get_current_port)
#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
start_dtnd -d -j0 -i0 -C mtcp:port=2342 -e 42 -r epidemic -n 1 -s mtcp://127.0.0.1:4223/node2 $DB1

PORT_NODE2=$(get_current_port)
#DB2="-W /tmp/node2 -D sled"
#DB2="-W /tmp/node2 -D sneakers"
start_dtnd -d -j0 -i0 -C mtcp:port=4223 -e 42 -r epidemic \
  -n 2 \
  -s mtcp://127.0.0.1:2342/node1 \
  -s mtcp://127.0.0.1:2432/node3 \
  $DB2

PORT_NODE3=$(get_current_port)
#DB3="-W /tmp/node3 -D sled"
#DB3="-W /tmp/node3 -D sneakers"
start_dtnd -d -j0 -i0 -C mtcp:port=2432 -e 42 -r epidemic -n 3 -s mtcp://127.0.0.1:4223/node2 $DB3

sleep 1

echo

echo "Sending 'test' to ipn:3.42"
echo test | $BINS/dtnsend -r ipn:3.42 -p $PORT_NODE1

sleep 1

echo -n "Receiving on node 3: "
$BINS/dtnrecv -v -e 42 -p $PORT_NODE3
RC=$?
echo "RET: $RC"
echo

wait_for_key $1

cleanup

exit $RC
