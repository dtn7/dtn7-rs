#!/bin/sh

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
    ./tests/local_nodes_dtn.sh &&
    ./tests/local_nodes_ipn.sh &&
    ./tests/local_ping_echo.sh &&
    ./tests/local_group_test.sh &&
    ./tests/local_trigger_test.sh &&
    ./tests/cla_chain_test.sh &&
    ./tests/ecla_test.sh &&
    ./tests/ecla_test_chain.sh &&
    ./tests/ecla_test_mtcp.sh &&
    ./tests/ecla_test_json_mtcp.sh &&
    ./tests/erouting_epidemic.sh &&
    ./tests/ecla_erouting_test_mtcp.sh &&
    ./tests/routing_saw.sh &&
    echo "SUCCESS"
