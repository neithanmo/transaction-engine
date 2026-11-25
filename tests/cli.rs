//! The CLI contract of the compiled binary: exit codes and stdout.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn run_binary(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_transaction-engine"))
        .args(args)
        .output()
        .expect("binary runs")
}

fn assert_fatal(output: &Output) {
    assert!(
        !output.status.success(),
        "expected a non-zero exit, got {}",
        output.status
    );
    assert!(
        output.stdout.is_empty(),
        "expected empty stdout, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(!output.stderr.is_empty(), "expected a message on stderr");
}

#[test]
fn no_argument_is_fatal() {
    assert_fatal(&run_binary(&[]));
}

#[test]
fn too_many_arguments_is_fatal() {
    let input = fixture("spec-example.input.csv");
    assert_fatal(&run_binary(&[&input, &input]));
}

#[test]
fn missing_file_is_fatal() {
    assert_fatal(&run_binary(&[&fixture("does-not-exist.csv")]));
}

#[test]
fn valid_file_prints_the_report_and_exits_zero() {
    let output = run_binary(&[&fixture("spec-example.input.csv")]);

    assert!(output.status.success(), "exit status {}", output.status);
    let expected = std::fs::read_to_string(fixture("spec-example.expected.csv"))
        .expect("expected report is readable");
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}
