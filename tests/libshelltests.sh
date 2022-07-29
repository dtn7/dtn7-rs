#!/bin/sh

echo "==> $(basename $0)"

# function to prepare binaries and test environment
function prepare_test() {
    if [ -z "$TARGET" ]; then
        export TARGET=release
    fi

    if [ $TARGET = "debug" ]; then
        cargo build --bins --examples
    else
        cargo build --release --bins --examples
    fi

    if [ $? -ne 0 ]; then
        echo "Build failed."
        exit 1
    fi
    export DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
    export BINS=$DIR/../target/$TARGET
}

function wait_for_key {
    if [[ $1 = "-k" ]]; then
        echo "Press any key to stop daemons and clean up logs"
        read -n 1
    else
        echo
        echo "Provide -k as parameter to keep session running."
        echo
    fi
}

CURNODE=1
function get_current_port {
    echo $((CURNODE - 1 + 3000))
}

function start_dtnd {
    OUT_NODE=$(mktemp /tmp/node$CURNODE.XXXXXX)
    # PORT is CURNODE -1 + 3000
    PORT_NODE=$((CURNODE - 1 + 3000))
    $DIR/../target/$TARGET/dtnd -w $PORT_NODE $@ 2>&1 &>$OUT_NODE &
    PID_NODE=$!
    echo node$CURNODE pid: $PID_NODE
    echo node$CURNODE out: $OUT_NODE
    echo node$CURNODE port: $PORT_NODE
    FILES="$FILES $OUT_NODE"
    PIDS="$PIDS $PID_NODE"
    CURNODE=$((CURNODE + 1))
}

function start_task {
    NAME=$1
    shift
    OUT_TASK=$(mktemp /tmp/$NAME.XXXXXX)
    "$BINS/$@" 2>&1 &>$OUT_TASK &
    PID_TASK=$!
    echo $NAME pid: $PID_TASK
    echo $NAME out: $OUT_TASK
    FILES="$FILES $OUT_TASK"
    PIDS="$PIDS $PID_TASK"
}

function cleanup {
    echo "Cleaning up..."
    kill $PIDS
    # if FILES is set, delete all files in it
    if [ -n "$FILES" ]; then
        rm $FILES
    fi
}
