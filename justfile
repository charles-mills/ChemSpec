set default-list

alias b := build
alias r := run
alias t := test

# Build the complete workspace
build:
    cargo build --workspace

# Build an optimized release binary
release:
    cargo build --workspace --release

# Run the ChemSpec application
run:
    cargo run -p chemspec-app

# Launch a fresh macOS bundle for Computer Use, or stop it
[positional-arguments]
agent-smoke mode="2d" reaction="alkali-water-lithium":
    ./packaging/scripts/agent-smoke-macos.sh "$1" "$2"

# Check the workspace without producing binaries
check:
    cargo check --workspace --all-targets

# Run all workspace tests
test:
    cargo test --workspace --all-targets

# Format the workspace
fmt:
    cargo fmt --all

# Check formatting without changing files
fmt-check:
    cargo fmt --all --check

# Run Clippy with the same settings as CI
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Validate the executable ChemSpec conformance contract
conformance:
    cargo run -p chems-conformance -- validate

# Run the complete local CI gate
ci: fmt-check test lint conformance
