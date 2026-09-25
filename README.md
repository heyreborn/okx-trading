# okx-trading

Rust implementation of the simulated OKX trading system described in [the architecture](docs/architecture.md). The workspace is currently an engineering foundation, not a trading application.

## Current packages

- [`crates/model`](crates/model/README.md): typed identities, exact units and minimal versioned facts.
- [`crates/config`](crates/config/README.md): role-aware deployment wiring.
- [`crates/instrument`](crates/instrument/README.md): pure read-only product and account directory validation.
- [`crates/okx-client`](crates/okx-client/README.md): GET-only OKX demo REST adapter and typed directory responses.
- [`crates/telemetry`](crates/telemetry/README.md): bounded process diagnostics.
- [`integration-tests`](integration-tests/README.md): isolated Compose and stage-one read-only catalog acceptance checks.

Stage-one contract and read-only directory acceptance passed for one Global demo account; scope and remaining regional/account-mode gaps are recorded in the [acceptance report](docs/stage1-acceptance.md). No runnable trading process or order path exists yet.

The virtual workspace lists implemented packages explicitly. Planned packages and processes are described in [the roadmap](docs/development-roadmap.md); they do not exist yet.

## Build

Use Rust 1.98.1 and run `cargo metadata --no-deps` and `cargo check --workspace --all-targets` from this directory. The [CI workflow](.github/workflows/ci.yml) runs formatting, Clippy, Nextest, doctests, cargo-deny, and rustdoc against the locked dependency graph. See [testing](docs/testing-strategy.md) and [deployment](docs/compose-deployment.md) for the later quality and infrastructure gates.

Changes to protected `main` go through a pull request and the required `quality` check. See the [GitHub CLI workflow](docs/gh-cli-workflow.md) for branch, PR, CI, and merge commands.
