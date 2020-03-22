#!/bin/bash
#
# sink.sh example called from dtntrigger
#
# Print received messages. Does some sanity checks...
#
# modified from ibr-dtn example

MAXPREVIEW=250

#First parameter is source EID, second is path to payload
src=$1
payload=$2

toprint=$(head -c $MAXPREVIEW $payload  | strings -n 1 )
actualsize=$(wc -c "$payload" | awk '{print $1}' )

echo -n "$src said: $toprint"
if [ "$actualsize" -gt "$MAXPREVIEW" ]
then
        echo -n "..."
fi
echo " ($actualsize bytes)"

