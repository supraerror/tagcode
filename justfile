# tagcode — justfile
# https://github.com/casey/just
#
# Run `just` with no arguments to list all recipes.

# List all available recipes.
default:
    @just --list

# Run every test: unit tests, cross-implementation checks, and doctests.
test:
    cargo test

# Run only the doctests (the runnable examples in the `///` doc comments).
doctest:
    cargo test --doc

# Build and open full, cross-linked API docs in the browser.
doc:
    cargo doc --open --no-deps

# Build the docs without opening a browser (useful in CI / headless envs).
doc-build:
    cargo doc --no-deps

# Run the dependency-free micro-benchmark comparing all 4 implementations.
bench:
    cargo run --release --example bench

# Run the heavy benchmark: multiple sizes (64B..2MiB) x multiple content
# profiles, with min/median/mean/max and MiB/s throughput. Takes a while.
heavy-bench:
    cargo run --release --example heavy_bench

# Format the codebase.
fmt:
    cargo fmt

# Check formatting without modifying files (CI-friendly).
fmt-check:
    cargo fmt -- --check

# Run clippy lints, denying warnings. Requires the clippy component.
lint:
    cargo clippy --all-targets -- -D warnings

# Type-check everything quickly without producing binaries.
check:
    cargo check --all-targets

# Build the optimized release artifacts.
build:
    cargo build --release

# Remove all build artifacts.
clean:
    cargo clean

# Run the full quality gate: formatting, lints, type-check, and tests.
# Use this before committing / opening a PR.
ci: fmt-check lint check test

# Quick end-to-end smoke test: encode/decode a string via every strategy,
# including the meta dispatcher, and print the results.
demo:
    cargo run --quiet --example demo
