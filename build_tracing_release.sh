#!/bin/sh

RUSTFLAGS="--cfg tokio_unstable" cargo build --release --features tracing
