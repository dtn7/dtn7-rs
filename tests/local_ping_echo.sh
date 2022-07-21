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

wait_for_key $1

#kill $PID_ECHO1
#rm $OUT_ECHO1

cleanup

exit $RC
