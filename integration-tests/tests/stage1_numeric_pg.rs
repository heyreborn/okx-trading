//! Explicit isolated PostgreSQL NUMERIC(38,18) round-trip check for the
//! stage-one exact-decimal domain. No persistent test table is created.

use model::units::Decimal;
use std::process::Command;

#[test]
#[ignore = "requires an isolated okx-stage1-test-* Compose PostgreSQL project"]
fn numeric_38_18_round_trip() {
    let project = std::env::var("OKX_TEST_COMPOSE_PROJECT")
        .expect("set an isolated stage-one Compose project");
    assert!(
        project.starts_with("okx-stage1-test-")
            && project
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    );
    let container = format!("{project}-postgres-1");
    for input in [
        "0",
        "0.000000000000000001",
        "-0.000000000000000001",
        "99999999999999999999.999999999999999999",
        "-99999999999999999999.999999999999999999",
    ] {
        let amount = Decimal::parse(input).expect("domain value");
        let sql = format!(
            "SELECT CAST('{}' AS NUMERIC(38,18))::text",
            amount.to_numeric_text()
        );
        let output = Command::new("docker")
            .args([
                "exec",
                &container,
                "psql",
                "-U",
                "okx_dev",
                "-d",
                "okx_trading_dev",
                "-Atc",
                &sql,
            ])
            .output()
            .expect("Docker CLI");
        assert!(
            output.status.success(),
            "PostgreSQL NUMERIC round trip failed"
        );
        let received = String::from_utf8(output.stdout).expect("numeric text");
        assert_eq!(Decimal::parse(received.trim()).expect("PG decimal"), amount);
    }
    let overflow = Command::new("docker")
        .args([
            "exec",
            &container,
            "psql",
            "-v",
            "ON_ERROR_STOP=1",
            "-U",
            "okx_dev",
            "-d",
            "okx_trading_dev",
            "-Atc",
            "SELECT CAST('100000000000000000000' AS NUMERIC(38,18))",
        ])
        .output()
        .expect("Docker CLI");
    assert!(
        !overflow.status.success(),
        "PostgreSQL must reject out-of-domain amount"
    );
}
