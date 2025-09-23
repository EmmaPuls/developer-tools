use anyhow::Result;
use crate::utils::{git, ui};

/// Execute the checkout command - equivalent to gitcheckout.sh
pub fn execute(branch_name: String, branch_flag: bool, dry_run: bool, git_args: Vec<String>) -> Result<()> {
    // If -b flag is used, just pass through to git checkout normally
    if branch_flag {
        if dry_run {
            ui::dry_run(&format!("Would execute: git checkout -b {}", branch_name));
            return Ok(());
        }
        
        let mut args = vec!["-b".to_string(), branch_name];
        args.extend(git_args);
        git::git_checkout(&args)?;
        return Ok(());
    }

    // Ensure we're in a git repository
    let repo = git::ensure_git_repo()?;

    if dry_run {
        return execute_dry_run(&repo, &branch_name);
    }

    // Check if the branch exists on the remote
    if git::remote_branch_exists(&branch_name)? {
        // Remote branch exists, now check if we already have a local branch
        if git::local_branch_exists(&repo, &branch_name)? {
            // Local branch exists, just switch to it
            git::switch_to_local_branch(&branch_name)?;
            ui::success(&format!("Switched to existing local branch '{}'", branch_name));
        } else {
            // No local branch exists, create one tracking the remote
            ui::info("Remote branch found. Creating local tracking branch...");
            git::git_checkout_new_branch(&branch_name, Some(&format!("origin/{}", branch_name)))?;
            ui::success(&format!("Successfully created local branch tracking remote '{}'", branch_name));
        }
    } else {
        // Branch doesn't exist on remote, create a new local branch
        ui::new_item(&format!("Branch '{}' doesn't exist remotely. Creating new local branch...", branch_name));
        git::git_checkout_new_branch(&branch_name, None)?;
        ui::success(&format!("Created new local branch '{}'", branch_name));
    }

    Ok(())
}

/// Execute dry-run mode to show what would happen
fn execute_dry_run(repo: &git2::Repository, branch_name: &str) -> Result<()> {
    ui::dry_run("Checking branch status...");

    // Check local branch existence
    let local_exists = git::local_branch_exists(repo, branch_name)?;
    if local_exists {
        ui::dry_run(&format!("Local branch '{}' exists", branch_name));
    } else {
        ui::dry_run(&format!("Local branch '{}' does not exist", branch_name));
    }

    // Check remote branch existence
    let remote_exists = git::remote_branch_exists(branch_name)?;
    if remote_exists {
        ui::dry_run(&format!("Remote branch 'origin/{}' exists", branch_name));
    } else {
        ui::dry_run(&format!("Remote branch 'origin/{}' does not exist", branch_name));
    }

    // Show what would happen
    match (local_exists, remote_exists) {
        (true, _) => {
            ui::dry_run(&format!("Would switch to existing local branch '{}'", branch_name));
        }
        (false, true) => {
            ui::dry_run(&format!("Would create local tracking branch for 'origin/{}'", branch_name));
        }
        (false, false) => {
            ui::dry_run(&format!("Would create new local branch '{}'", branch_name));
        }
    }

    Ok(())
}