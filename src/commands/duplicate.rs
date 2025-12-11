use anyhow::{anyhow, Result};
use crate::utils::{git, ui};
use dialoguer::Input;
use regex::Regex;
use std::process::Command;

/// Execute the duplicate-stack command
pub fn execute(num_commits: u32, verbose: bool, no_stash: bool) -> Result<()> {
    // Ensure we're in a git repository
    let _repo = git::ensure_git_repo()?;

    debug_log(verbose, &format!("Arguments parsed: num_commits={}, verbose={}, no_stash={}", 
        num_commits, verbose, no_stash));

    // Validate that the input is a positive integer
    debug_log(verbose, "Validating that num_commits is a positive integer");
    if num_commits == 0 {
        ui::error("number_of_commits must be a positive integer");
        return Err(anyhow!("Invalid number of commits"));
    }
    debug_log(verbose, "✓ num_commits validation passed");

    // Verify we have enough commits to undo
    debug_log(verbose, "Counting available commits");
    let commit_count = get_commit_count()?;
    debug_log(verbose, &format!("Total commits in current branch: {}", commit_count));
    
    if num_commits > commit_count {
        ui::error(&format!(
            "Cannot undo {} commits. Only {} commits available.",
            num_commits, commit_count
        ));
        return Err(anyhow!("Not enough commits"));
    }
    debug_log(verbose, "✓ Sufficient commits available");

    let mut commit_messages = Vec::new();

    if !no_stash {
        ui::info(&format!("Undoing the last {} commits and stashing each one...", num_commits));
        println!();

        // Process each commit
        for i in 1..=num_commits {
            debug_log(verbose, &format!("[{}/{}] Starting iteration", i, num_commits));

            // Get the commit message before undoing
            debug_log(verbose, &format!("[{}/{}] Retrieving current commit message", i, num_commits));
            let commit_message = get_current_commit_message()?;
            debug_log(verbose, &format!("[{}/{}] Current commit message: '{}'", i, num_commits, commit_message));

            // Store commit message
            commit_messages.push(commit_message.clone());
            debug_log(verbose, &format!("[{}/{}] Stored in commit_messages array at index {}", i, num_commits, i - 1));

            println!("[{}/{}] Undoing commit: {}", i, num_commits, commit_message);

            // Undo the commit using git undocommit
            debug_log(verbose, &format!("[{}/{}] Executing: git undocommit", i, num_commits));
            undo_commit()?;
            debug_log(verbose, &format!("[{}/{}] ✓ Commit undone", i, num_commits));

            // Stash with the commit message as the stash name
            debug_log(verbose, &format!("[{}/{}] Executing: git stash -u -m {}", i, num_commits, commit_message));
            stash_changes(&commit_message)?;
            debug_log(verbose, &format!("[{}/{}] ✓ Changes stashed", i, num_commits));

            println!("  ✓ Stashed as: \"{}\"", commit_message);
            println!();
        }

        debug_log(verbose, "All commit messages stored in commit_messages array");
        debug_log(verbose, &format!("Array contents: {:?}", commit_messages));

        ui::success(&format!("Successfully undone {} commits and created stashes.", num_commits));
        println!();
        debug_log(verbose, "Retrieving stash list");
        println!("Your stashes:");
        show_recent_stashes(num_commits)?;

        // Remove JIRA ticket IDs from commit messages
        debug_log(verbose, "Processing commit messages to remove JIRA ticket IDs");
        println!();
        println!("Cleaned commit messages (JIRA IDs removed):");
        println!();

        let cleaned_messages = clean_commit_messages(&commit_messages, verbose, num_commits);

        for (i, msg) in cleaned_messages.iter().enumerate() {
            println!("  {} {}", i + 1, msg);
        }
    } else {
        debug_log(verbose, "NO_STASH flag is set, skipping stashing stage");
        ui::info("Skipping stashing stage...");
    }

    // Get current branch name
    debug_log(verbose, "Retrieving current branch name");
    let current_branch = get_current_branch()?;
    debug_log(verbose, &format!("Current branch: {}", current_branch));

    // Suggest new branch name
    let suggested_branch_name = format!("{}-new", current_branch);
    debug_log(verbose, &format!("Suggested branch name: {}", suggested_branch_name));

    println!();
    println!("Ready to create a new branch.");
    println!();

    // Prompt user for branch name
    let user_branch_name: String = Input::new()
        .with_prompt(format!("Enter new branch name (default: {})", suggested_branch_name))
        .allow_empty(true)
        .interact_text()?;

    // Use suggested name if user just pressed Enter
    let branch_name = if user_branch_name.is_empty() {
        debug_log(verbose, &format!("User pressed Enter, using suggested branch name: {}", suggested_branch_name));
        suggested_branch_name
    } else {
        debug_log(verbose, &format!("User entered custom branch name: {}", user_branch_name));
        user_branch_name
    };

    // Validate that branch name has stack/ prefix
    debug_log(verbose, "Validating branch name has 'stack/' prefix");
    if !branch_name.starts_with("stack/") {
        ui::error(&format!("Branch name must start with 'stack/' prefix. Provided: {}", branch_name));
        return Err(anyhow!("Invalid branch name format"));
    }
    debug_log(verbose, "✓ Branch name validation passed");

    println!();
    println!("Creating new branch: {}", branch_name);
    debug_log(verbose, &format!("Executing: git checkout -b {}", branch_name));

    create_new_branch(&branch_name)?;
    debug_log(verbose, "✓ New branch created successfully");

    ui::success(&format!("Successfully created branch: {}", branch_name));

    // Now pop stashes and recreate commits with cleaned messages
    if !no_stash {
        println!();
        ui::info(&format!("Popping {} stashes and creating commits...", num_commits));
        println!();

        // Get the cleaned commit messages
        let cleaned_messages = clean_commit_messages(&commit_messages, verbose, num_commits);

        // Pop stashes in reverse order (LIFO - Last In First Out)
        // The stashes were created in order 1, 2, 3... so stash@{0} is the last one
        // We need to pop in reverse: stash@{num_commits-1}, stash@{num_commits-2}, ..., stash@{0}
        for i in (0..num_commits).rev() {
            let idx = i as usize;
            let iteration = (num_commits - i) as usize;
            debug_log(verbose, &format!("[{}/{}] Starting stash pop iteration", iteration, num_commits));

            // Pop the stash (always use stash@{0} because each pop shifts the list)
            debug_log(verbose, &format!("[{}/{}] Executing: git stash pop", iteration, num_commits));
            pop_stash()?;
            debug_log(verbose, &format!("[{}/{}] ✓ Stash popped", iteration, num_commits));

            // Stage all changes
            debug_log(verbose, &format!("[{}/{}] Executing: git add -A", iteration, num_commits));
            stage_all_changes()?;
            debug_log(verbose, &format!("[{}/{}] ✓ Changes staged", iteration, num_commits));

            // Create commit with cleaned message
            let cleaned_msg = &cleaned_messages[idx];
            println!("[{}/{}] Creating commit: {}", iteration, num_commits, cleaned_msg);
            debug_log(verbose, &format!("[{}/{}] Executing: git commit -m \"{}\"", iteration, num_commits, cleaned_msg));
            create_commit(cleaned_msg)?;
            debug_log(verbose, &format!("[{}/{}] ✓ Commit created", iteration, num_commits));
            println!("  ✓ Created commit: {}", cleaned_msg);
            println!();
        }

        ui::success(&format!("Successfully created {} commits on branch: {}", num_commits, branch_name));
    }

    debug_log(verbose, "✓ Script execution completed successfully");

    Ok(())
}

/// Debug logging function
fn debug_log(verbose: bool, message: &str) {
    if verbose {
        eprintln!("[DEBUG] {}", message);
    }
}

/// Get the current commit message
fn get_current_commit_message() -> Result<String> {
    let output = Command::new("git")
        .args(["log", "-1", "--pretty=format:%s"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("Failed to get commit message"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Undo the last commit
fn undo_commit() -> Result<()> {
    let output = Command::new("git")
        .args(["undocommit"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git undocommit failed: {}", stderr));
    }

    Ok(())
}

/// Stash changes with a message
fn stash_changes(message: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["stash", "-u", "-m", message])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git stash failed: {}", stderr));
    }

    Ok(())
}

/// Get the total number of commits in the current branch
fn get_commit_count() -> Result<u32> {
    let output = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("Failed to count commits"));
    }

    let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    count_str
        .parse::<u32>()
        .map_err(|_| anyhow!("Failed to parse commit count"))
}

/// Show the most recent stashes
fn show_recent_stashes(num_commits: u32) -> Result<()> {
    let output = Command::new("git")
        .args(["stash", "list"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("Failed to list stashes"));
    }

    let stashes = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stashes.lines().take(num_commits as usize).collect();
    for line in lines {
        println!("{}", line);
    }

    Ok(())
}

/// Clean commit messages by removing JIRA ticket IDs
fn clean_commit_messages(messages: &[String], verbose: bool, num_commits: u32) -> Vec<String> {
    // Regex to match JIRA ticket IDs at the start: LETTERS/NUMBERS-NUMBERS followed by optional whitespace
    let jira_regex = Regex::new(r"^[A-Z0-9]+-[0-9]+[\s]*").unwrap();

    messages
        .iter()
        .enumerate()
        .map(|(i, original_message)| {
            debug_log(verbose, &format!("[{}/{}] Original message: '{}'", i + 1, num_commits, original_message));

            // Remove JIRA ticket ID prefix
            let cleaned_message = jira_regex
                .replace(original_message, "")
                .to_string();

            debug_log(verbose, &format!("[{}/{}] Cleaned message: '{}'", i + 1, num_commits, cleaned_message));

            cleaned_message
        })
        .collect()
}

/// Get the current branch name
fn get_current_branch() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()?;

    if !output.status.success() {
        return Err(anyhow!("Failed to get current branch"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Create a new branch
fn create_new_branch(branch_name: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["checkout", "-b", branch_name])
        .status()?;

    if !status.success() {
        return Err(anyhow!("Failed to create new branch"));
    }

    Ok(())
}

/// Pop a stash (uses stash@{0}, the most recent one)
fn pop_stash() -> Result<()> {
    let output = Command::new("git")
        .args(["stash", "pop"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git stash pop failed: {}", stderr));
    }

    Ok(())
}

/// Stage all changes
fn stage_all_changes() -> Result<()> {
    let output = Command::new("git")
        .args(["add", "-A"])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("git add failed: {}", stderr));
    }

    Ok(())
}

/// Create a commit with the given message
fn create_commit(message: &str) -> Result<()> {
    let status = Command::new("ag")
        .args(["commit", "-m", message])
        .status()?;

    if !status.success() {
        return Err(anyhow!("ag commit failed with exit code: {}", status.code().unwrap_or(-1)));
    }

    Ok(())
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

        temp_dir
    }

    fn create_commits(temp_dir: &TempDir, count: usize) {
        for i in 0..count {
            let filename = format!("file{}.txt", i);
            std::fs::write(temp_dir.path().join(&filename), format!("content {}", i)).unwrap();
            
            StdCommand::new("git")
                .args(["add", &filename])
                .current_dir(&temp_dir)
                .output()
                .unwrap();

            StdCommand::new("git")
                .args(["commit", "-m", &format!("Commit {}", i)])
                .current_dir(&temp_dir)
                .output()
                .unwrap();
        }
    }

    #[test]
    fn test_get_commit_count() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            // Create 3 commits
            create_commits(&temp_repo, 3);
            
            let count = get_commit_count().unwrap();
            assert_eq!(count, 3, "Should have 3 commits");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_get_current_commit_message() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            let message = get_current_commit_message().unwrap();
            assert_eq!(message, "Commit 0", "Should get the correct commit message");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_clean_commit_messages_removes_jira_ids() {
        let messages = vec![
            "PROJ-123 Fix the bug".to_string(),
            "ABC-456 Add new feature".to_string(),
            "No JIRA ID here".to_string(),
            "TICKET-789 Multiple words in message".to_string(),
        ];

        let cleaned = clean_commit_messages(&messages, false, 4);

        assert_eq!(cleaned[0], "Fix the bug");
        assert_eq!(cleaned[1], "Add new feature");
        assert_eq!(cleaned[2], "No JIRA ID here");
        assert_eq!(cleaned[3], "Multiple words in message");
    }

    #[test]
    fn test_clean_commit_messages_with_spaces() {
        let messages = vec![
            "PROJ-123   Extra spaces".to_string(),
            "ABC-456\tTab character".to_string(),
        ];

        let cleaned = clean_commit_messages(&messages, false, 2);

        assert_eq!(cleaned[0], "Extra spaces");
        assert_eq!(cleaned[1], "Tab character");
    }

    #[test]
    fn test_get_current_branch() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            let branch = get_current_branch().unwrap();
            // Git creates 'master' or 'main' by default
            assert!(!branch.is_empty(), "Should get a branch name");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_create_new_branch() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            create_new_branch("stack/test-branch").unwrap();
            
            let current_branch = get_current_branch().unwrap();
            assert_eq!(current_branch, "stack/test-branch", "Should be on the new branch");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_stash_changes() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            // Make a change and stage it
            std::fs::write(temp_repo.path().join("test.txt"), "test content").unwrap();
            StdCommand::new("git")
                .args(["add", "test.txt"])
                .current_dir(&temp_repo)
                .output()
                .unwrap();

            stash_changes("Test stash message").unwrap();
            
            // Verify stash was created
            let output = StdCommand::new("git")
                .args(["stash", "list"])
                .current_dir(&temp_repo)
                .output()
                .unwrap();
            
            let stash_list = String::from_utf8_lossy(&output.stdout);
            assert!(stash_list.contains("Test stash message"), "Stash should be created with message");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_debug_log_verbose() {
        // This test just ensures debug_log doesn't panic
        debug_log(true, "Test message");
        debug_log(false, "Test message");
    }

    #[test]
    fn test_undo_commit_requires_git_alias() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 2);
            
            // This will fail because we haven't set up the git alias
            // But it proves the function is being called correctly
            let undo_result = undo_commit();
            assert!(undo_result.is_err(), "undo_commit should fail without git undocommit alias");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_get_commit_count_empty_repo() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            let count = get_commit_count().unwrap();
            assert_eq!(count, 0, "Empty repo should have 0 commits");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_show_recent_stashes() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            // Create some stashes
            std::fs::write(temp_repo.path().join("file1.txt"), "content1").unwrap();
            StdCommand::new("git")
                .args(["add", "file1.txt"])
                .current_dir(&temp_repo)
                .output()
                .unwrap();
            stash_changes("Stash 1").unwrap();
            
            std::fs::write(temp_repo.path().join("file2.txt"), "content2").unwrap();
            StdCommand::new("git")
                .args(["add", "file2.txt"])
                .current_dir(&temp_repo)
                .output()
                .unwrap();
            stash_changes("Stash 2").unwrap();
            
            let result = show_recent_stashes(2);
            assert!(result.is_ok(), "Should show recent stashes");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_pop_stash() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            // Create a stash
            std::fs::write(temp_repo.path().join("test.txt"), "test content").unwrap();
            StdCommand::new("git")
                .args(["add", "test.txt"])
                .current_dir(&temp_repo)
                .output()
                .unwrap();
            stash_changes("Test stash").unwrap();
            
            // Verify stash exists
            let output = StdCommand::new("git")
                .args(["stash", "list"])
                .current_dir(&temp_repo)
                .output()
                .unwrap();
            let stash_list_before = String::from_utf8_lossy(&output.stdout);
            assert!(stash_list_before.contains("Test stash"), "Stash should exist before pop");
            
            // Pop the stash
            pop_stash().unwrap();
            
            // Verify file was restored
            let content = std::fs::read_to_string(temp_repo.path().join("test.txt")).unwrap();
            assert_eq!(content, "test content", "File should be restored after pop");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_stage_all_changes() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            // Make a change
            std::fs::write(temp_repo.path().join("test.txt"), "test content").unwrap();
            
            // Stage it
            stage_all_changes().unwrap();
            
            // Verify it's staged
            let output = StdCommand::new("git")
                .args(["status", "--porcelain"])
                .current_dir(&temp_repo)
                .output()
                .unwrap();
            let status = String::from_utf8_lossy(&output.stdout);
            assert!(status.contains("A  test.txt") || status.contains("M  test.txt"), 
                "File should be staged");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }

    #[test]
    fn test_create_commit() {
        let temp_repo = create_test_repo();
        let original_dir = std::env::current_dir().unwrap();

        let result = std::panic::catch_unwind(|| {
            std::env::set_current_dir(&temp_repo).unwrap();
            
            create_commits(&temp_repo, 1);
            
            // Make and stage a change
            std::fs::write(temp_repo.path().join("test.txt"), "test content").unwrap();
            stage_all_changes().unwrap();
            
            // Create commit
            create_commit("Test commit message").unwrap();
            
            // Verify commit was created
            let message = get_current_commit_message().unwrap();
            assert_eq!(message, "Test commit message", "Commit should be created with correct message");
        });

        let _ = std::env::set_current_dir(original_dir);
        assert!(result.is_ok(), "Test should not panic");
    }
}
