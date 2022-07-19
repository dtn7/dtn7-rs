#!/bin/sh

cargo test --release && \
./tests/local_nodes_dtn.sh && \
./tests/local_nodes_ipn.sh && \
./tests/local_ping_echo.sh && \
./tests/local_group_test.sh && \
./tests/local_trigger_test.sh && \
./tests/cla_chain_test.sh && \
./tests/ecla_test.sh && \
./tests/ecla_test_chain.sh && \
./tests/ecla_test_mtcp.sh && \
./tests/ecla_test_json_mtcp.sh && \
./tests/erouting_epidemic.sh && \
./tests/ecla_erouting_test_mtcp.sh && \
echo "SUCCESS"