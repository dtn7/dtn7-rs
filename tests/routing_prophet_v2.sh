#!/bin/bash

# PRoPHET v2 on a static three-node line: node1 <-> node2 <-> node3.
# Periodic information exchanges must propagate a stable predictability gradient,
# after which GRTR can carry a singleton bundle through node2.

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

PROPHET_OPTS="-r prophet_v2 -R prophet_v2.tick_interval=500ms -R prophet_v2.exchange_interval=2s -R prophet_v2.i_typ=10s -R prophet_v2.time_unit=1s -R prophet_v2.record_lifetime=5s"

PORT_NODE1=$(get_current_port)
start_dtnd -d -j2s -i0 -C tcp:port=2342 -e incoming $PROPHET_OPTS -n node1 -s tcp://127.0.0.1:4223/node2

PORT_NODE2=$(get_current_port)
start_dtnd -d -j2s -i0 -C tcp:port=4223 -e incoming $PROPHET_OPTS -n node2 -s tcp://127.0.0.1:2342/node1 -s tcp://127.0.0.1:2432/node3

PORT_NODE3=$(get_current_port)
start_dtnd -d -j2s -i0 -C tcp:port=2432 -e incoming $PROPHET_OPTS -n node3 -s tcp://127.0.0.1:4223/node2

echo
echo "Waiting for periodic predictability exchanges"
sleep 10

GETDATA=$(curl -s "http://127.0.0.1:$PORT_NODE1/routing/getdata")
echo "node1 PRoPHET state: $GETDATA"
if echo "$GETDATA" | jq -e '.local.node2 > .local.node3 and .local.node3 > 0' >/dev/null; then
  echo "node1 has a direct-to-transitive gradient towards node3"
  GRADIENT_OK=0
else
  echo "node1 has no valid gradient towards node3"
  GRADIENT_OK=1
fi

echo
echo "Sending 'test' to node3"
echo test | $BINS/dtnsend -r dtn://node3/incoming -p $PORT_NODE1
sleep 8

function destination_count {
  $BINS/dtnquery -p $1 bundles --addr "$2" | grep "dtn://" | wc -l | awk '{print $1}'
}

NODE1_BUNDLES=$(destination_count $PORT_NODE1 dtn://node3/incoming)
NODE2_BUNDLES=$(destination_count $PORT_NODE2 dtn://node3/incoming)
NODE3_BUNDLES=$(destination_count $PORT_NODE3 dtn://node3/incoming)

echo "Application bundles at node1/node2/node3: $NODE1_BUNDLES/$NODE2_BUNDLES/$NODE3_BUNDLES"
if [ "$NODE1_BUNDLES" = "1" ] && [ "$NODE2_BUNDLES" = "0" ] && [ "$NODE3_BUNDLES" = "1" ]; then
  STORE_OK=0
else
  STORE_OK=1
fi

echo -n "Receiving on node3: "
timeout 5 $BINS/dtnrecv -v -e incoming -p $PORT_NODE3
RECV_OK=$?
echo

wait_for_key $1
cleanup

if [ "$GRADIENT_OK" = "0" ] && [ "$STORE_OK" = "0" ] && [ "$RECV_OK" = "0" ]; then
  exit 0
else
  exit 1
fi
