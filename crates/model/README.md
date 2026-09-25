# model

Owns stable cross-package contract primitives. `src/lib.rs` exports the initial schema marker and `identity` module. DEV-008 and DEV-009 will add units and facts. There is no account, order, or market fact model yet.

`identity` validates each namespace-specific ID at construction and JSON decoding. The wire form is a string, preserving case and bytes; IDs are nonempty, ASCII, at most 128 bytes, and contain letters, digits, `-`, `_`, `.`, or `:` after an alphanumeric first character. Different ID types cannot be passed interchangeably. A collection owner must still reject duplicate IDs and check cross-reference integrity. Validation is pure: no I/O, state, retries, or transaction boundary. It does not check account permission, OKX existence, or environment. The schema marker does not validate or migrate envelopes.

Python reference: `../okx-trading-py/src/okx_trading/domain/contracts.py` and `tests/test_contracts.py` encode typed contract identifiers and reject incompatible envelope versions. Rust intentionally uses separate ID types and tests JSON round trips, invalid inputs, and duplicate detection in `identity.rs`. Schema-version checks remain the owning message contract's responsibility; wire compatibility beyond string identity and database round trips remain unverified.

See [DEV-007](../../docs/development-roadmap.md), [architecture](../../docs/architecture.md), and [contract rules](../../docs/contracts-and-invariants.md). This package depends only on Serde and has no OKX API access.
