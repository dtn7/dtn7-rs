#!/bin/sh

# check if running on macos and use gdate if available
if [ "$(uname)" = "Darwin" ] && command -v gdate >/dev/null 2>&1; then
  date_cmd="gdate"
else
  date_cmd="date"
fi

TS_NOW=$($date_cmd +%s.%3N)
#TS_NOW=$(date +%s)
echo "$TS_NOW | INCOMING $1 $2"