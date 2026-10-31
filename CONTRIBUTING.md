# Contributing to dtn7-rs

## Setup

Besides a stable Rust toolchain, development uses:

- [just](https://github.com/casey/just) to run common tasks,
- [prek](https://github.com/j178/prek) to run formatters and linters as Git hooks,
- optionally [uv](https://github.com/astral-sh/uv) for `just lint`, and a nightly toolchain with [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) for `just fuzz`.

Install the required tools and set up the Git hooks:

```sh
cargo install --locked just prek
prek install
```

The Git hooks run formatting checks, clippy, rustdoc, typo checks, and TOML/YAML validation before each commit.
Before each push, they run the tests.

## Common tasks

Run `just` to list all recipes.
The most important ones:

| Recipe                  | Purpose                                                                                                       |
| ----------------------- | ------------------------------------------------------------------------------------------------------------- |
| `just fix`              | Apply clippy and rustfmt fixes in place.                                                                      |
| `just lint`             | Check formatting, clippy and rustdoc without changing files.                                                  |
| `just check`            | Type-check all targets.                                                                                       |
| `just test`             | Run the unit, integration and doc tests.                                                                      |
| `just test-integration` | Run the shell-based tests in `tests/`, which start local `dtnd` instances. They need `curl`, `jq` and `perl`. |
| `just fuzz [seconds]`   | Run every fuzz target for the given time (default 30 s).                                                      |
| `just prek`             | Run all prek hooks on all files.                                                                              |

The fuzz targets in `fuzz/` are a separate Cargo workspace, so a plain `cargo` command at the root skips them.
The recipes always cover both workspaces.

CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) runs the same recipes for every pull request and every push to `master`.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/), e.g. `fix(tcpcl): ...` or `feat!: ...`.
The changelog is generated from them with [git-cliff](https://git-cliff.org/).

## Releases

Maintainers release with `./release.sh vX.Y.Z`, which updates the versions and the changelog, then commits and tags the release.
Pushing the tag starts [`.github/workflows/cd.yml`](.github/workflows/cd.yml), which creates the GitHub release with binaries for Linux and macOS and publishes the crate to crates.io.
If it fails, it can be run again for the same tag from the Actions tab, using the current version of the workflow.
