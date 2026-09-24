//! Real Compose connectivity checks for PG, Kafka, and S3. Test inputs come
//! only from an explicitly named isolated Compose project.

use std::env;
use std::process::{Command, Output};

fn container(project: &str, service: &str) -> String {
    format!("{project}-{service}-1")
}

fn exec(container: &str, args: &[&str]) -> Output {
    Command::new("docker")
        .arg("exec")
        .arg(container)
        .args(args)
        .output()
        .expect("Docker CLI must be available")
}

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "container command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("service output must be UTF-8")
}

#[test]
fn isolated_services_accept_roundtrips() {
    let project = env::var("OKX_TEST_COMPOSE_PROJECT")
        .expect("set OKX_TEST_COMPOSE_PROJECT to an isolated okx-stage0-test-* project");
    assert!(
        project.starts_with("okx-stage0-test-")
            && project
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "refusing a non-test Compose project"
    );

    let pg = container(&project, "postgres");
    let kafka = container(&project, "kafka");
    let s3 = container(&project, "object-store");

    assert_eq!(
        successful(exec(
            &pg,
            &[
                "psql",
                "-U",
                "okx_dev",
                "-d",
                "okx_trading_dev",
                "-Atc",
                "SELECT 1"
            ]
        ))
        .trim(),
        "1"
    );
    assert!(
        successful(exec(
            &kafka,
            &[
                "/opt/kafka/bin/kafka-topics.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--list",
            ]
        ))
        .lines()
        .any(|topic| topic == "okx-market-dev")
    );

    let marker = include_str!("../fixtures/connectivity-v1.txt").trim();
    let put = format!(
        "printf %s {marker} | curl --fail --silent --show-error --aws-sigv4 'aws:amz:us-east-1:s3' --user \"$AWS_ACCESS_KEY_ID:$AWS_SECRET_ACCESS_KEY\" --request PUT --data-binary @- http://127.0.0.1:8333/okx-archive-dev/stage0-integration.txt --output /dev/null"
    );
    successful(exec(&s3, &["sh", "-c", &put]));
    let get = "curl --fail --silent --show-error --aws-sigv4 'aws:amz:us-east-1:s3' --user \"$AWS_ACCESS_KEY_ID:$AWS_SECRET_ACCESS_KEY\" http://127.0.0.1:8333/okx-archive-dev/stage0-integration.txt";
    assert_eq!(successful(exec(&s3, &["sh", "-c", get])), marker);
}
