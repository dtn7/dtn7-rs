#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#STATUS_REPORTS="-g"

PORT_NODE1=$(get_current_port)
#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
start_dtnd -d -j5s -i0 -C mtcp:port=2342 -e incoming -r epidemic -n node1 -s mtcp://127.0.0.1:4223/node2 $DB1

PORT_NODE2=$(get_current_port)
#DB2="-W /tmp/node2 -D sled"
#DB2="-W /tmp/node2 -D sneakers"
start_dtnd -d -j5s -i0 -C mtcp:port=4223 -e incoming -r epidemic \
  -n node2 \
  -s mtcp://127.0.0.1:2342/node1 \
  $DB2

sleep 1

echo "Starting dtntrigger on node 2"
CMD='echo INCOMING: '
start_task dtntrigger dtntrigger -p $PORT_NODE2 -e incoming -c "$CMD" -v
# get last word from string
OUT_TRIGGER=$(echo $FILES | awk '{print $NF}')
echo $OUT_TRIGGER
sleep 1

echo

echo "Sending 'test' to node 2"
echo test | $BINS/dtnsend -r dtn://node2/incoming -p $PORT_NODE1

sleep 1

echo "Sending 'test2' to node 2"
echo test2 | $BINS/dtnsend -r dtn://node2/incoming -p $PORT_NODE1

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

wait_for_key $1

cleanup

exit $RC
