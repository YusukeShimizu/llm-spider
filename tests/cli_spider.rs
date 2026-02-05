use predicates::prelude::*;

#[test]
fn spider_help_includes_reasoning_effort_flag() {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("llm-spider");
    cmd.args(["spider", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--reasoning-effort"));
}
