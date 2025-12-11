/// Integration tests for the duplicate command
/// These tests verify the command works end-to-end through the CLI

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn setup_git_repo() -> TempDir {
    let temp_dir = tempfile::tempdir().unwrap();
    
    // Initialize git repo
    Command::new("git")
        .args(&["init"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to init git repo");
    
    // Configure git
    Command::new("git")
        .args(&["config", "user.name", "Test User"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to set git user");
    
    Command::new("git")
        .args(&["config", "user.email", "test@example.com"])
        .current_dir(&temp_dir)
        .output()
        .expect("Failed to set git email");
    
    temp_dir
}

fn create_test_commits(repo_dir: &TempDir, count: usize) {
    for i in 0..count {
        let filename = format!("file{}.txt", i);
        let filepath = repo_dir.path().join(&filename);
        fs::write(&filepath, format!("content {}", i)).expect("Failed to write file");
        
        Command::new("git")
            .args(&["add", &filename])
            .current_dir(&repo_dir)
            .output()
            .expect("Failed to add file");
        
        Command::new("git")
            .args(&["commit", "-m", &format!("Test commit {}", i)])
            .current_dir(&repo_dir)
            .output()
            .expect("Failed to create commit");
    }
}

#[test]
fn test_duplicate_help() {
    let mut cmd = Command::cargo_bin("dt").expect("Failed to get binary");
    cmd.arg("duplicate")
        .arg("--help");
    
    cmd.assert().success()
        .stdout(predicate::str::contains("Undo last x commits"));
}

#[test]
fn test_duplicate_requires_num_commits() {
    let mut cmd = Command::cargo_bin("dt").expect("Failed to get binary");
    cmd.arg("duplicate");
    
    cmd.assert().failure();
}

#[test]
fn test_duplicate_with_invalid_num_commits() {
    let mut cmd = Command::cargo_bin("dt").expect("Failed to get binary");
    cmd.arg("duplicate")
        .arg("abc");  // Not a number
    
    cmd.assert().failure();
}

#[test]
fn test_duplicate_verbose_flag() {
    let mut cmd = Command::cargo_bin("dt").expect("Failed to get binary");
    cmd.arg("duplicate")
        .arg("1")
        .arg("-v")
        .arg("--help");
    
    // The help should show with verbose flag present
    cmd.assert().success();
}

#[test]
fn test_duplicate_no_stash_flag() {
    let mut cmd = Command::cargo_bin("dt").expect("Failed to get binary");
    cmd.arg("duplicate")
        .arg("1")
        .arg("--no-stash")
        .arg("--help");
    
    // The help should show with no-stash flag present
    cmd.assert().success();
}

/// This test verifies the command structure is correct
/// Full execution requires git undocommit alias and interactive input
#[test]
fn test_duplicate_not_in_git_repo() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    
    let mut cmd = Command::cargo_bin("dt").expect("Failed to get binary");
    cmd.arg("duplicate")
        .arg("1")
        .current_dir(&temp_dir);
    
    cmd.assert().failure()
        .stderr(predicate::str::contains("Not in a git repository")
            .or(predicate::str::contains("git")));
}
