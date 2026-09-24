# okx-trading

Rust implementation of the simulated OKX trading system described in [the architecture](docs/architecture.md). The workspace is currently an engineering foundation, not a trading application.

## Current packages

- [`crates/model`](crates/model/README.md): version marker for future cross-package contracts.

The virtual workspace lists implemented packages explicitly. Planned packages and processes are described in [the roadmap](docs/development-roadmap.md); they do not exist yet.

## Build

Use Rust 1.98.1 and run `cargo metadata --no-deps` and `cargo check --workspace --all-targets` from this directory. See [testing](docs/testing-strategy.md) and [deployment](docs/compose-deployment.md) for the later quality and infrastructure gates.
