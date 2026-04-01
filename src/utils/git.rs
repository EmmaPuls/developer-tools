use anyhow::{anyhow, Result};
use git2::{BranchType, Repository};
use std::process::Command;

/// Check if we're in a git repository
pub fn ensure_git_repo() -> Result<Repository> {
    Repository::discover(".").map_err(|_| anyhow!("Not in a git repository"))
}

/// Check if a branch exists locally
pub fn local_branch_exists(repo: &Repository, branch_name: &str) -> Result<bool> {
    match repo.find_branch(branch_name, BranchType::Local) {
        Ok(_) => Ok(true),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(e) => Err(anyhow!("Error checking local branch: {}", e)),
    }
}

/// Check if a branch exists on remote without fetching it
pub fn remote_branch_exists(branch_name: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["ls-remote", "--heads", "origin", branch_name])
        .output()?;

    // If output is not empty, the branch exists
    Ok(!output.stdout.is_empty())
}

/// Execute git checkout with the given arguments
pub fn git_checkout(args: &[String]) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("checkout");
    cmd.args(args);

    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow!("git checkout failed"));
    }

    Ok(())
}

/// Execute git checkout -b with the given arguments
pub fn git_checkout_new_branch(branch_name: &str, tracking_branch: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(["checkout", "-b", branch_name]);

    if let Some(tracking) = tracking_branch {
        cmd.arg(tracking);
    }

    let status = cmd.status()?;
    if !status.success() {
        return Err(anyhow!("git checkout -b failed"));
    }

    Ok(())
}

/// Switch to an existing local branch
pub fn switch_to_local_branch(branch_name: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["checkout", branch_name])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "Failed to switch to local branch '{}'",
            branch_name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::TempDir;

    fn create_test_repo() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();

        StdCommand::new("git")
            .args(["init"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        std::fs::write(temp_dir.path().join("README.md"), "test").unwrap();
        StdCommand::new("git")
            .args(["add", "README.md"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        StdCommand::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&temp_dir)
            .output()
            .unwrap();

        temp_dir
    }

    #[test]
    fn test_ensure_git_repo() {
        let temp_repo = create_test_repo();

        // Change to temp repo directory
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_repo).unwrap();

        let result = ensure_git_repo();
        assert!(result.is_ok());

        // Restore original directory
        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_ensure_git_repo_failure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let result = ensure_git_repo();
        assert!(result.is_err());

        std::env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn test_local_branch_exists() {
        let temp_repo = create_test_repo();

        // Open the repo directly by path — avoids mutating the process working directory
        // (which is shared across parallel tests and causes flaky failures).
        let repo = git2::Repository::open(temp_repo.path()).unwrap();

        // Get the actual default branch name from git
        let output = StdCommand::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&temp_repo)
            .output()
            .unwrap();
        let default_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Test existing branch (whatever the default is)
        let exists_default = local_branch_exists(&repo, &default_branch).unwrap();
        assert!(
            exists_default,
            "Default branch '{}' should exist",
            default_branch
        );

        // Test non-existing branch
        let exists_nonexistent = local_branch_exists(&repo, "nonexistent-branch").unwrap();
        assert!(!exists_nonexistent);
    }

    #[test]
    fn test_git_functions_basic() {
        // Test that functions can be called without panicking
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        // Use closure to ensure cleanup happens
        let test_result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();

            // Test git_checkout_new_branch with simple branch name
            let result = git_checkout_new_branch("simple-test", None);
            assert!(result.is_ok(), "Should create new branch");

            // Test that we can call switch_to_local_branch (may fail, but shouldn't panic)
            let _result = switch_to_local_branch("simple-test");

            // Test git_checkout with basic args
            let args = vec!["simple-test".to_string()];
            let _result = git_checkout(&args);

            // Test remote_branch_exists safely
            let _result = remote_branch_exists("nonexistent");
        });

        // Always restore directory
        let _ = std::env::set_current_dir(original_dir);

        if test_result.is_err() {
            panic!("Test failed");
        }
    }

    #[test]
    fn test_error_conditions() {
        // Test error handling
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let test_result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();

            // Test git_checkout_new_branch with invalid tracking
            let result = git_checkout_new_branch("error-test", Some("invalid/ref"));
            assert!(result.is_err(), "Should fail with invalid tracking branch");

            // Test switch_to_local_branch with nonexistent branch
            let result = switch_to_local_branch("definitely-does-not-exist");
            assert!(result.is_err(), "Should fail for nonexistent branch");

            // Test git_checkout with invalid arguments
            let invalid_args = vec!["--this-flag-does-not-exist".to_string()];
            let result = git_checkout(&invalid_args);
            assert!(result.is_err(), "Should fail with invalid args");
        });

        // Always restore directory
        let _ = std::env::set_current_dir(original_dir);

        if test_result.is_err() {
            panic!("Test failed");
        }
    }
}
