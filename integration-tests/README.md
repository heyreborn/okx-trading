# integration-tests

Owns real cross-service tests against an isolated Compose project. `src/lib.rs` documents the package boundary. `tests/connectivity.rs` invokes service clients inside the explicitly named Compose containers to verify a PostgreSQL query, the initialized Kafka topic, and signed S3 PUT/GET. `fixtures/connectivity-v1.txt` contains only a public marker; no account, market, or credential data is stored here.

The test requires `OKX_TEST_COMPOSE_PROJECT=okx-stage0-test-<suffix>` and refuses other project names. Start `deploy/compose/compose.yaml` under that project and run the `kafka-init` profile before `cargo test -p integration-tests --test connectivity`. The test uses Docker CLI and the containers' bundled clients, so it validates live service protocols but does not exercise Rust SQLx, rdkafka, or object-store adapters. Those packages do not exist yet. The test writes a repeatable object key and has no transaction or message offset ownership.

See [DEV-004](../docs/development-roadmap.md), [testing strategy](../docs/testing-strategy.md), and [deployment](../docs/compose-deployment.md). This package has no Python behavior to preserve and no OKX API access. Multi-node recovery and authorization by application roles remain unverified.
