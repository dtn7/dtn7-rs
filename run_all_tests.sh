#!/bin/sh

cargo test --release && \
./tests/local_nodes_dtn.sh && \
./tests/local_nodes_ipn.sh && \
./tests/local_ping_echo.sh && \
./tests/local_trigger_test.sh && \
echo "SUCCESS"