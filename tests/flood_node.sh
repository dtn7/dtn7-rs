#!/bin/sh

cargo build --release --bin dtnsend

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
NUM=10

if [ $# -gt 0 ]; then
    NUM=$1
fi

for i in `seq 1 $NUM`; do
    echo $i
    RECEIVER=$(tr -dc a-z </dev/urandom | head -c 12 ; echo '')
    tr -dc A-Za-z0-9 </dev/urandom | head -c 64 ; echo '' | $DIR/../target/release/dtnsend -r dtn://$RECEIVER/incoming
done