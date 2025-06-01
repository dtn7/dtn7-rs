#!/bin/bash

DATASIZE=1000000 # 1MB
NUM_BUNDLES=1000

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
start_task dtntrigger dtntrigger -p $PORT_NODE2 -e incoming -c ./bench_sink.sh -v
# get last word from string
OUT_TRIGGER=$(echo $FILES | awk '{print $NF}')
echo $OUT_TRIGGER
sleep 1

echo


echo "Generating dummy data of size $DATASIZE bytes"
OUT_FILE=$(mktemp /tmp/abc-script.XXXXXX)
dd if=/dev/zero of=$OUT_FILE bs=1 count=$DATASIZE

TS_NOW=$(date +%s)
echo "Timestamp: $TS_NOW"
echo "Sending $NUM_BUNDLES bundles to node 2"
for i in $(seq 1 $NUM_BUNDLES); do
  echo "Sending bundle $i"
  $BINS/dtnsend -r dtn://node2/incoming -p $PORT_NODE1 $OUT_FILE
done

sleep 2
echo

cat $OUT_TRIGGER

echo



NUM_RECV=$(cat $OUT_TRIGGER | grep INCOMING | wc -l)
echo "Received on node 2: $NUM_RECV"
if [ $NUM_RECV -ne $NUM_BUNDLES ]; then
  RC=1
else
  RC=0
fi
echo
echo "RET: $RC"
echo

LAST_MODIFIED=$(date -r $OUT_TRIGGER +%s)
LAST_RECEIVED=$(cat $OUT_TRIGGER | grep INCOMING | tail -n1 | cut -d " " -f1)
echo "Last received bundle: $LAST_RECEIVED"
echo Last modified timestamp: $LAST_MODIFIED
echo Start timestamp: $TS_NOW
DATASIZE=$((DATASIZE * NUM_BUNDLES))
echo "Time of all transfers ($DATASIZE bytes): $((LAST_MODIFIED - TS_NOW)) seconds"
BW=$((DATASIZE / (LAST_MODIFIED - TS_NOW)))
BW_BITS=$((BW * 8))

# Turn bandwidth into human-readable format
if [ $BW_BITS -ge 1000000 ]; then
  BW_BITS=$(echo "scale=2; $BW_BITS / 1000000" | bc)
  echo "Bandwidth: $BW_BITS MBit/sec"
else
  BW_BITS=$(echo "scale=2; $BW_BITS / 1000" | bc)
  echo "Bandwidth: $BW_BITS KBit/sec"
fi

wait_for_key $1

rm $OUT_FILE
cleanup

exit $RC
