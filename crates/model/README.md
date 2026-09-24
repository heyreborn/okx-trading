# model

Owns stable cross-package contract primitives. Currently `src/lib.rs` exports only `INITIAL_SCHEMA_VERSION`; DEV-007 through DEV-009 will add identity, units, and fact modules. There is no account, order, or market model yet.

The constant is a compile-time marker with no I/O, state, retries, or transaction boundary. It does not validate or migrate serialized data. No Python behavior is being ported by this scaffold. The version is checked by a local unit test; protocol compatibility and database round trips remain unverified.

See [DEV-001](../../docs/development-roadmap.md), [architecture](../../docs/architecture.md), and [contract rules](../../docs/contracts-and-invariants.md). This package has no external dependencies or OKX API access.
