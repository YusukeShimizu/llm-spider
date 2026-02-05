use predicates::prelude::*;

#[test]
fn hello_without_name_prints_world() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.args(["hello"])
        .assert()
        .success()
        .stdout("Hello, world!\n");
}

#[test]
fn hello_with_name_prints_name() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.args(["hello", "--name", "Alice"])
        .assert()
        .success()
        .stdout("Hello, Alice!\n");
}

#[test]
fn rust_log_debug_emits_debug_line_to_stderr() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.env("RUST_LOG", "debug")
        .args(["hello"])
        .assert()
        .success()
        .stderr(predicate::str::contains("parsed cli"));
}

#[test]
fn spider_help_includes_reasoning_effort_flag() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.args(["spider", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--reasoning-effort"));
}
