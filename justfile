# The main workspace and the cargo-fuzz crate, which is its own workspace.
manifests := "Cargo.toml fuzz/Cargo.toml"

default:
    @just --list

# Fuzz without --all, which would also format core/dtn7 through its dependency.
# Run code formatting tools.
fmt *ARGS:
    cargo fmt --all {{ARGS}}
    cargo fmt --manifest-path fuzz/Cargo.toml {{ARGS}}

# Run code quality checks.
check *ARGS:
    for m in {{manifests}}; do cargo check --manifest-path $m --workspace --all-targets {{ARGS}} || exit 1; done

# Run cargo clippy.
clippy *ARGS:
    for m in {{manifests}}; do cargo clippy --manifest-path $m --workspace --all-targets {{ARGS}} || exit 1; done

# Build the documentation.
doc:
    cargo doc --workspace --no-deps --document-private-items

# Apply clippy and rustfmt fixes in place.
fix: (clippy "--fix" "--allow-dirty") fmt

# Run code linting tools without modifying files.
lint: (fmt "--check")
    CARGO_BUILD_WARNINGS=deny just clippy --keep-going
    RUSTDOCFLAGS="-D warnings" just doc
    uvx tombi lint

# Run the unit, integration and doc tests.
test:
    cargo test --workspace --all-targets
    cargo test --workspace --doc

# Run the shell-based tests that start local dtnd instances (release build).
test-integration:
    ./run_all_tests.sh

# Run every fuzz target for the given number of seconds (needs nightly and cargo-fuzz).
# The target is passed explicitly,
# since a prebuilt cargo-fuzz defaults to the musl target it was built for,
# which the sanitizers do not support.
fuzz seconds="30":
    host=$(rustc +nightly -vV | sed -n 's/^host: //p'); for t in $(cargo +nightly fuzz list); do cargo +nightly fuzz run --target "$host" "$t" -- -max_total_time={{seconds}} || exit 1; done

# Run all prek hooks on all files.
prek:
    prek run --all-files
