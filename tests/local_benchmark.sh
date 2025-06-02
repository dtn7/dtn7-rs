#!/bin/bash

# DATASIZE=64000 # 64KB
# NUM_BUNDLES=1000 # 1k bundles

# DATASIZE=1000000 # 1MB
# NUM_BUNDLES=1000 # 1k bundles

DATASIZE=10000000 # 10MB
NUM_BUNDLES=100 # 100 bundles

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#STATUS_REPORTS="-g"
CLA=mtcp

PORT_NODE1=$(get_current_port)
#DB1="-W /tmp/node1 -D sled"
#DB1="-W /tmp/node1 -D sneakers"
start_dtnd -d -j5s -i0 -C $CLA:port=2342 -e incoming -r epidemic -n node1 -s $CLA://127.0.0.1:4223/node2 $DB1

PORT_NODE2=$(get_current_port)
#DB2="-W /tmp/node2 -D sled"
#DB2="-W /tmp/node2 -D sneakers"
start_dtnd -d -j5s -i0 -C $CLA:port=4223 -e incoming -r epidemic \
  -n node2 \
  -s $CLA://127.0.0.1:2342/node1 \
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
BS=1
DS=$DATASIZE
if [ $DATASIZE -gt 1000000 ]; then
  BS=1m
  DS=$((DATASIZE / 1000000))
elif [ $DATASIZE -gt 1000 ]; then
  BS=1k
  DS=$((DATASIZE / 1000))
fi
dd if=/dev/zero of=$OUT_FILE bs=$BS count=$DS

# get timestamp in seconds as float with ms
# check if running on macos and use gdate if available
if [ "$(uname)" = "Darwin" ] && command -v gdate >/dev/null 2>&1; then
  date_cmd="gdate"
else
  date_cmd="date"
fi

TS_NOW=$($date_cmd +%s.%3N)

echo "Timestamp: $TS_NOW"
echo "Sending $NUM_BUNDLES bundles to node 2"
for i in $(seq 1 $NUM_BUNDLES); do
  echo "Sending bundle $i"
  $BINS/dtnsend -r dtn://node2/incoming -p $PORT_NODE1 $OUT_FILE
done

echo "Waiting for bundles to be received on node 2"
MAX_WAIT=60
wait_for_bundles() {
  local timeout=$1
  local start_time=$(date +%s)
  while true; do
    if [ -f $OUT_TRIGGER ]; then
      NUM_RECV=$(cat $OUT_TRIGGER | grep INCOMING | wc -l)
      if [ $NUM_RECV = $NUM_BUNDLES ]; then
        return 0
      fi
    fi
    sleep 1
    local elapsed=$(( $(date +%s) - start_time ))
    if [ $elapsed -ge $timeout ]; then
      return 1
    fi
  done
}
wait_for_bundles $MAX_WAIT

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
TOTALDATASIZE=$((DATASIZE * NUM_BUNDLES))
TX_DURATION=$(echo "$LAST_RECEIVED - $TS_NOW" | bc)
echo "Time of all transfers ($NUM_BUNDLES x $DATASIZE = $TOTALDATASIZE bytes total): $TX_DURATION seconds"
BW=$(echo "$TOTALDATASIZE / $TX_DURATION" | bc)
BW_BITS=$((BW * 8))

# Turn bandwidth into human-readable format
if [ $BW_BITS -ge 1000000000 ]; then
  BW_BITS=$(echo "scale=2; $BW_BITS / 1000000000" | bc)
  echo "Bandwidth: $BW_BITS GBit/sec"
elif [ $BW_BITS -ge 1000000 ]; then
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
