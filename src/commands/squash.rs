use anyhow::{anyhow, Result};
use crate::utils::{git, ui};
use console::style;
use dialoguer::{Confirm, Input};
use std::process::Command;

/// Execute the squash command - equivalent to squash.sh
pub fn execute(count: u32, message: Option<String>, dry_run: bool, commit_args: Vec<String>) -> Result<()> {
    // Ensure we're in a git repository
    let _repo = git::ensure_git_repo()?;

    if dry_run {
        return execute_dry_run(count, message, &commit_args);
    }

    // Show what commits will be squashed
    show_commits_to_squash(count)?;

    // Get commit message
    let commit_message = get_commit_message(count, message)?;

    // Show additional commit options if any
    if !commit_args.is_empty() {
        println!("\n{}Additional commit options:{} {}", 
                style("Additional commit options:").bold(), "", commit_args.join(" "));
    }

    println!("\n{}Commit message:{} {}", 
            style("Commit message:").bold(), "", commit_message);

    // Confirm before proceeding
    if !Confirm::new()
        .with_prompt("Proceed with squash?")
        .default(true)
        .interact()? {
        ui::info("Squash cancelled");
        return Ok(());
    }

    // Perform the squash
    perform_squash(count, &commit_message, &commit_args)?;

    Ok(())
}

/// Execute dry-run mode to show what would happen
fn execute_dry_run(count: u32, message: Option<String>, commit_args: &[String]) -> Result<()> {
    ui::dry_run(&format!("Would squash the last {} commits", count));
    
    if let Some(msg) = message {
        ui::dry_run(&format!("Would use commit message: '{}'", msg));
    } else {
        ui::dry_run("Would prompt for commit message or use default from earliest commit");
    }
    
    if !commit_args.is_empty() {
        ui::dry_run(&format!("Would use additional commit options: {}", commit_args.join(" ")));
    }
    
    ui::dry_run("Would show commit preview and ask for confirmation");
    
    Ok(())
}

/// Show the commits that will be squashed
fn show_commits_to_squash(count: u32) -> Result<()> {
    println!("\n{}", 
            style(format!("You are about to squash the last {} commits.", count)).bold());
    
    let output = Command::new("git")
        .args(["log", "-n", &count.to_string(), "--pretty=format:%h - %an: %s"])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("Failed to get commit history"));
    }
    
    let commits = String::from_utf8_lossy(&output.stdout);
    println!("\n{}", style("Commit messages:").bold());
    println!("{}", commits);
    
    Ok(())
}

/// Get the commit message to use for the squashed commit
fn get_commit_message(count: u32, provided_message: Option<String>) -> Result<String> {
    if let Some(message) = provided_message {
        return Ok(message);
    }
    
    // Get the earliest commit message from the range to be squashed
    let output = Command::new("git")
        .args(["log", "--reverse", "-n", &count.to_string(), "--pretty=format:%s"])
        .output()?;
    
    if !output.status.success() {
        return Err(anyhow!("Failed to get earliest commit message"));
    }
    
    let commits = String::from_utf8_lossy(&output.stdout);
    let earliest_message = commits.lines().next()
        .unwrap_or("Squashed commits")
        .to_string();
    
    println!("\n{} {}", 
            style("Default commit message:").bold(), earliest_message);
    
    let user_message: String = Input::new()
        .with_prompt("Press Enter to use the default message or type a new one")
        .allow_empty(true)
        .interact_text()?;
    
    if user_message.is_empty() {
        Ok(earliest_message)
    } else {
        Ok(user_message)
    }
}

/// Perform the actual squash operation
fn perform_squash(count: u32, message: &str, commit_args: &[String]) -> Result<()> {
    println!("\n{}", style("Squashing commits...").bold());
    
    // Reset to HEAD~count
    let reset_output = Command::new("git")
        .args(["reset", "--soft", &format!("HEAD~{}", count)])
        .output()?;
    
    if !reset_output.status.success() {
        ui::error("git reset failed");
        return Err(anyhow!("git reset failed"));
    }
    
    ui::success("Reset successful");
    
    // Commit with the new message
    let mut commit_cmd = Command::new("git");
    commit_cmd.args(["commit", "-m", message]);
    commit_cmd.args(commit_args);
    
    let commit_output = commit_cmd.output()?;
    
    if !commit_output.status.success() {
        ui::error("git commit failed");
        ui::info("Resetting to previous state...");
        
        // Reset to the previous state
        let _ = Command::new("git")
            .args(["reset", "--hard", "HEAD@{1}"])
            .output();
        
        return Err(anyhow!("git commit failed"));
    }
    
    println!("\n{}", style("Squash complete!").bold());
    println!("Latest commit message: {}", message);
    
    Ok(())
}