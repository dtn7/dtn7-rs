#!/bin/bash

. $(dirname $(perl -MCwd -e 'print Cwd::abs_path shift' "$0"))/libshelltests.sh

prepare_test

#STATUS_REPORTS="-g"

start_dtnd -d -i0 -r epidemic -n node1

sleep 2

start_task echo1 examples/dtnecho2 -v

echo
sleep 1

echo "Sending 3 pings to node1"
$BINS/examples/dtnping -d 'dtn://node1/echo' -c 6 -t 500ms

RC=$?
echo "RET: $RC"

NUM_BUNDLES=$($BINS/dtnquery bundles | grep "dtn://" | wc -l | awk '{print $1}')
NUM_DELETED=$($BINS/dtnquery store | grep -o "Deleted" | wc -l | awk '{print $1}')

EXPECTED_BUNDLES=0
EXPECTED_DELETED=12

echo "Bundles in store on node 1: : $NUM_BUNDLES / $EXPECTED_BUNDLES"
echo "Bundles marked as Deleted in store on node 1: : $NUM_DELETED / $EXPECTED_DELETED"
if [ "$NUM_BUNDLES" = "$EXPECTED_BUNDLES" ] && [ "$NUM_DELETED" = "$EXPECTED_DELETED" ]; then
  echo "Correct number of bundles in store!"
else
  echo "Incorrect number of bundles in store!"
  RC=1
fi


wait_for_key $1

#kill $PID_ECHO1
#rm $OUT_ECHO1

cleanup

exit $RC
