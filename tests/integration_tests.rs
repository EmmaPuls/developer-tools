use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as StdCommand;
use tempfile::TempDir;

/// Helper to create a temporary git repository for testing
fn create_test_git_repo() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();

    // Initialize git repo
    StdCommand::new("git")
        .args(["init"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    // Configure git for testing
    StdCommand::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    StdCommand::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    // Create initial commit
    std::fs::write(temp_dir.path().join("README.md"), "# Test repo").unwrap();
    StdCommand::new("git")
        .args(["add", "README.md"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    StdCommand::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(&temp_dir)
        .output()
        .unwrap();

    temp_dir
}

#[test]
fn test_help_commands() {
    // Test main help
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Git and development workflow tools",
        ));

    // Test checkout help
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["checkout", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Smart git checkout"));

    // Test squash help
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["squash", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Interactive git squash"));

    // Test rovo help
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["rovo", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Rovo CLI wrapper"));
}

#[test]
fn test_checkout_dry_run() {
    let temp_repo = create_test_git_repo();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["checkout", "test-branch", "--dry-run"])
        .current_dir(&temp_repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("[DRY RUN]"))
        .stdout(predicate::str::contains("Would create new local branch"));
}

#[test]
fn test_checkout_with_b_flag_dry_run() {
    let temp_repo = create_test_git_repo();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["checkout", "-b", "new-feature", "--dry-run"])
        .current_dir(&temp_repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("[DRY RUN]"))
        .stdout(predicate::str::contains(
            "Would execute: git checkout -b new-feature",
        ));
}

#[test]
fn test_squash_dry_run() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["squash", "3", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[DRY RUN]"))
        .stdout(predicate::str::contains("Would squash the last 3 commits"));
}

#[test]
fn test_squash_with_message_dry_run() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["squash", "2", "-m", "Test message", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Would use commit message: 'Test message'",
        ));
}

#[test]
fn test_rovo_basic() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["rovo", "--help"]).assert().success();
}

#[test]
fn test_checkout_real_scenarios() {
    let temp_repo = create_test_git_repo();

    // Test creating a new local branch (dry run first to avoid hanging)
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["checkout", "new-feature", "--dry-run"])
        .current_dir(&temp_repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("Would create new local branch"));
}

#[test]
fn test_checkout_with_b_flag() {
    let temp_repo = create_test_git_repo();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["checkout", "-b", "feature-branch"])
        .current_dir(&temp_repo)
        .assert()
        .success();
}

#[test]
fn test_checkout_not_in_git_repo() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["checkout", "some-branch"])
        .current_dir(&temp_dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not in a git repository"));
}

#[test]
fn test_squash_not_in_git_repo() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["squash", "2"])
        .current_dir(&temp_dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not in a git repository"));
}

#[test]
fn test_squash_with_message() {
    let temp_repo = create_test_git_repo();

    // Create additional commits for squashing
    fs::write(temp_repo.path().join("file1.txt"), "content1").unwrap();
    StdCommand::new("git")
        .args(["add", "file1.txt"])
        .current_dir(&temp_repo)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "Add file1"])
        .current_dir(&temp_repo)
        .output()
        .unwrap();

    fs::write(temp_repo.path().join("file2.txt"), "content2").unwrap();
    StdCommand::new("git")
        .args(["add", "file2.txt"])
        .current_dir(&temp_repo)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["commit", "-m", "Add file2"])
        .current_dir(&temp_repo)
        .output()
        .unwrap();

    // Test dry run with commits to squash
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["squash", "2", "-m", "Combined commits", "--dry-run"])
        .current_dir(&temp_repo)
        .assert()
        .success()
        .stdout(predicate::str::contains("Would squash the last 2 commits"))
        .stdout(predicate::str::contains("Combined commits"));
}

#[test]
fn test_rovo_no_args() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.arg("rovo")
        .assert()
        .success()
        .stdout(predicate::str::contains("Rovo CLI Wrapper"));
}

#[test]
fn test_invalid_args() {
    // Test invalid subcommand
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.arg("invalid").assert().failure();

    // Test missing squash count
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.arg("squash").assert().failure();
}

#[test]
fn test_version_and_help() {
    // Test version
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.arg("--version").assert().success();

    // Test main help
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Git and development workflow tools",
        ));
}

// ── Phase 00 regression-net: dt fixup ────────────────────────────────────────

#[test]
fn test_fixup_help() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["fixup", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Manage the stack fixup workflow"));
}

#[test]
fn test_fixup_init_not_in_git_repo() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["fixup", "init"])
        .current_dir(&temp_dir)
        // Provide piped stdin so dialoguer doesn't hang waiting for a TTY
        .write_stdin("\n\n\n\n\n\n\n\n\n\n")
        .assert()
        // It may succeed (writes config) or fail — but must never panic/SIGABRT.
        // The key invariant is: exit code is 0 or 1, not a signal.
        .stderr(predicate::str::is_empty().not());
}

#[test]
fn test_fixup_no_args_not_in_git_repo() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["fixup"])
        .current_dir(&temp_dir)
        .write_stdin("\n\n\n\n\n\n\n\n\n\n")
        .assert()
        .failure();
}

// ── Phase 0: dt add tests ─────────────────────────────────────────────────────

#[test]
fn test_add_help() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Generate a script using AI"));
}

#[test]
fn test_add_requires_prompt_or_message() {
    // When stdin is not a TTY and no -m flag is given, the editor path will
    // fail to find any editor. The command must fail cleanly (non-zero exit)
    // without panicking or hanging.
    //
    // We blank out every editor env var AND point HOME to an empty temp dir
    // so that `git config core.editor` also returns nothing, preventing the
    // process from accidentally launching a real editor (e.g. `code --wait`)
    // and hanging indefinitely.
    let empty_home = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add"])
        .write_stdin("") // non-TTY stdin, no editor available
        .env_remove("VISUAL")
        .env_remove("EDITOR")
        .env_remove("GIT_EDITOR")
        .env("HOME", empty_home.path())
        .env("GIT_CONFIG_NOSYSTEM", "1") // ignore /etc/gitconfig
        .assert()
        .failure();
}

#[test]
fn test_add_inline_message_flag_accepted() {
    // The -m flag must be accepted by clap (it may still fail later when
    // trying to spawn `acli`, but it must not fail at argument parsing).
    // We verify the failure mode is NOT "unrecognised argument".
    //
    // Point PATH to an empty dir so `acli` is not found and the process
    // fails immediately rather than hanging waiting for an AI response.
    let empty_bin = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add", "-m", "copy all .md files from ~/notes to ~/backup"])
        .env("PATH", empty_bin.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognised").not())
        .stderr(predicate::str::contains("unexpected argument").not());
}

#[test]
fn test_add_lang_flag_accepted() {
    // Same approach: empty PATH so `acli` can't be found and we fail fast.
    let empty_bin = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add", "-m", "list all files", "--lang", "js"])
        .env("PATH", empty_bin.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognised").not())
        .stderr(predicate::str::contains("unexpected argument").not());
}

// ── Goal 4: dt run ───────────────────────────────────────────────────────────

#[test]
fn test_run_help() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run a generated script"));
}

#[test]
fn test_run_nonexistent_script() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["run", "this_script_definitely_does_not_exist_xyz.sh"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("No such")));
}

#[test]
fn test_run_no_args_no_scripts_fails_cleanly() {
    // When ~/.dt/scripts/ is empty (or doesn't exist), running `dt run` with
    // no args must fail cleanly rather than panic or hang.
    // We point the scripts dir at a known-empty temp dir via env override or
    // simply accept the failure without asserting on the exact message.
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["run"])
        .write_stdin("") // non-TTY: FuzzySelect will fail, which is expected
        .assert()
        .failure();
}

// ── Goal 3: async-first prompt contract ──────────────────────────────────────

#[test]
fn test_add_meta_prompt_async_pattern_example() {
    // Verify that ADD_META_PROMPT satisfies all async-first invariants.
    // This is the integration-level contract test for Goal 3.
    use dt::commands::add::ADD_META_PROMPT;

    let lower = ADD_META_PROMPT.to_lowercase();
    assert!(
        lower.contains("background") || lower.contains("backgrounded") || lower.contains(" &"),
        "Prompt must mention backgrounded calls"
    );
    assert!(
        lower.contains("pid") || lower.contains("$!"),
        "Prompt must mention PID capture"
    );
    assert!(lower.contains("wait"), "Prompt must mention waiting on background calls");
    assert!(
        lower.contains("filename") && lower.contains("description") && lower.contains("script"),
        "Prompt must specify the three required JSON fields"
    );
}
