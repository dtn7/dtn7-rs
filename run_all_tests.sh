#!/bin/bash

filter_output() {
    printf '==> %-40s' "$(basename $1)"
    if [ -z "$VERBOSE" ]; then
        OUT=$($1 2>&1)
        RET=$?
        if [ $RET -ne 0 ]; then
            echo "fail: $RET"
            echo "$OUT"
            exit 1
        else
            echo "ok"
        fi
    else
        echo
        $1
        RET=$?
        if [ $RET -ne 0 ]; then
            echo "===> fail: $RET"
            echo "$OUT"
            exit 1
        else
            echo "===> ok"
        fi
    fi
}
# check if variable TARGET is set
if [ -z "$TARGET" ]; then
    export TARGET=release
fi
if [ $TARGET = "debug" ]; then
    TARGET_OPT=""
else
    TARGET_OPT=--release
fi
cargo test $TARGET_OPT &&
    filter_output ./tests/local_nodes_dtn.sh &&
    filter_output ./tests/local_nodes_ipn.sh &&
    filter_output ./tests/local_ping_echo.sh &&
    filter_output ./tests/local_group_test.sh &&
    filter_output ./tests/local_trigger_test.sh &&
    filter_output ./tests/local_nodes_dtn_httppull.sh &&
    filter_output ./tests/local_nodes_http_dtn.sh &&
    filter_output ./tests/store_delete.sh &&
    filter_output ./tests/lifetime.sh &&
    filter_output ./tests/cla_chain_test.sh &&
    filter_output ./tests/ecla_test.sh &&
    filter_output ./tests/ecla_test_chain.sh &&
    filter_output ./tests/ecla_test_mtcp.sh &&
    filter_output ./tests/ecla_test_json_mtcp.sh &&
    filter_output ./tests/erouting_epidemic.sh &&
    filter_output ./tests/ecla_erouting_test_mtcp.sh &&
    filter_output ./tests/routing_saw.sh &&
    echo "SUCCESS"
