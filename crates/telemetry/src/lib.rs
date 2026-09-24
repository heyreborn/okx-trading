//! Process observability primitives. Logs and metrics aid diagnosis, while
//! account decisions and audit facts must remain in the authoritative ledger.
//! See the [package README](../README.md).

pub mod health;
pub mod log;
pub mod metrics;
