use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};
use dialoguer::{Confirm, Input, Select};
use dirs::home_dir;
use regex::Regex;
use serde::Deserialize;

use crate::utils::{git, ui};

const CONFIG_FILENAME: &str = ".dt-fixup-config";

#[derive(Clone)]
struct FixupOptions {
    author_name: String,
    stack_pattern: String,
    pull_request_keyword: String,
    acli_message: String,
    acli_output_file: String,
    acli_output_schema: String,
    acli_config_file: String,
    stack_log_limit: u32,
    verbose: bool,
    default_message: String,
}

impl Default for FixupOptions {
    fn default() -> Self {
        Self {
            author_name: "Emma Puls".to_string(),
            stack_pattern: "^stack/".to_string(),
            pull_request_keyword: "Pull Request".to_string(),
            acli_message: "Look at the staged changes in git and provide a commit message that would be appropriate for these changes. The message should be concise but descriptive, ideally starting with a verb in the present tense.".to_string(),
            acli_output_file:
                "../fixup/resources/acli_rovodev_output.json".to_string(),
            acli_output_schema:
                "../fixup/resources/acli_schema.json".to_string(),
            acli_config_file:
                "../fixup/resources/config.ag-stack.yml".to_string(),
            stack_log_limit: 10,
            verbose: false,
            default_message: "Update stack commit".to_string(),
        }
    }
}

#[derive(Clone)]
struct StackCommit {
    sha: String,
    subject: String,
}

#[derive(Deserialize)]
struct AcliOutput {
    commit_message: Option<String>,
}

enum ConfigInitMode {
    Auto,
    Manual,
}

/// Execute the fixup command, creating the config file on first run if needed.
pub fn run_fixup() -> Result<()> {
    let existing = read_config()?;
    let options = match existing {
        Some(cfg) => cfg,
        None => {
            ui::info("First-time setup: creating fixup resources directory");
            setup_fixup_resources()?;
            prompt_and_write_config(ConfigInitMode::Auto, None)?
        }
    };

    execute(options)
}

/// Interactive config initialization accessible via `dt fixup init`.
pub fn run_init() -> Result<()> {
    let existing = read_config()?;
    
    ui::info("Setting up fixup resources directory");
    setup_fixup_resources()?;
    
    prompt_and_write_config(ConfigInitMode::Manual, existing)?;
    ui::success(&format!(
        "Configuration stored at {}",
        config_path()?.display()
    ));
    Ok(())
}

/// Execute the fixup workflow - equivalent to ag-stack-update.sh
fn execute(options: FixupOptions) -> Result<()> {
    git::ensure_git_repo()?;

    ui::info("Validating current branch matches stack pattern");
    let stack_regex = Regex::new(&options.stack_pattern)
        .with_context(|| format!("Invalid STACK_PATTERN '{}'.", options.stack_pattern))?;
    require_stack_branch(&stack_regex)?;

    ui::info("Checking for staged or unstaged changes");
    ensure_changes_present()?;

    if options.stack_log_limit == 0 {
        return Err(anyhow!("STACK_LOG_LIMIT must be a positive integer."));
    }

    ui::info(&format!(
        "Collecting candidate stack commits (last {} commits)",
        options.stack_log_limit
    ));
    let commits = collect_stack_commits(&options)?;

    ui::info("Prompting for target commit selection");
    let selected = select_stack_commit(&commits)?;
    ui::info(&format!(
        "Selected commit {} - {}",
        abbreviate_sha(&selected.sha),
        selected.subject
    ));

    ui::info("Staging working tree changes");
    stage_changes()?;

    ui::info("Gathering acli rovodev inputs");
    verify_acli_inputs(&options)?;

    let acli_message = resolve_acli_message(&options.acli_message)?;

    ui::info("Requesting commit message suggestion from acli rovodev");
    request_commit_message(&options, &acli_message)?;

    ui::info("Extracting suggested commit message");
    let suggested_message =
        extract_commit_message(&options)?.unwrap_or_else(|| options.default_message.clone());

    ui::info("Showing git status summary");
    show_git_status()?;

    if !Confirm::new()
        .with_prompt("Review the diff now if needed. Continue?")
        .default(true)
        .interact()?
    {
        return Err(anyhow!("Aborting at user request."));
    }

    ui::info(&format!(
        "Creating fixup commit targeting {}",
        abbreviate_sha(&selected.sha)
    ));
    create_fixup_commit(&selected.sha)?;

    ui::info("Running autosquash rebase");
    run_autosquash_rebase(&selected.sha)?;

    print_suggested_message(&suggested_message);
    let final_message = prompt_final_message(&suggested_message)?;

    ui::info("Submitting stack via ag");
    submit_stack(&final_message)?;

    ui::success("Stack commit updated and submitted.");
    Ok(())
}

fn require_stack_branch(pattern: &Regex) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("Failed to read current branch")?;

    if !output.status.success() {
        return Err(anyhow!("Unable to detect current git branch."));
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if !pattern.is_match(&branch) {
        return Err(anyhow!(
            "Current branch '{}' does not look like a stack",
            branch
        ));
    }

    Ok(branch)
}

fn ensure_changes_present() -> Result<()> {
    let unstaged = git_diff_has_changes(&["--quiet"])?;
    let staged = git_diff_has_changes(&["--cached", "--quiet"])?;

    if !staged && !unstaged {
        return Err(anyhow!(
            "No staged or unstaged changes detected. Nothing to commit."
        ));
    }

    if unstaged {
        let proceed = Confirm::new()
            .with_prompt("Unstaged changes detected. Continue with these changes?")
            .default(true)
            .interact()?;

        if !proceed {
            return Err(anyhow!("Aborting at user request."));
        }
    }

    Ok(())
}

fn collect_stack_commits(options: &FixupOptions) -> Result<Vec<StackCommit>> {
    let log_output = Command::new("git")
        .args([
            "log",
            "-n",
            &options.stack_log_limit.to_string(),
            "--pretty=format:%H%x1f%an%x1f%s",
            "HEAD",
        ])
        .output()
        .context("Failed to read git log")?;

    if !log_output.status.success() {
        return Err(anyhow!("git log failed"));
    }

    let mut commits = Vec::new();
    let stdout = String::from_utf8_lossy(&log_output.stdout);

    for line in stdout.lines() {
        let mut parts = line.split('\u{1f}');
        let sha = match parts.next() {
            Some(value) if !value.is_empty() => value.to_string(),
            _ => continue,
        };
        let author = parts.next().unwrap_or_default().to_string();
        let subject = parts.next().unwrap_or_default().to_string();

        log_verbose(
            options.verbose,
            &format!(
                "Inspecting commit {} (author={}) subject={}",
                abbreviate_sha(&sha),
                author,
                subject
            ),
        );

        let body_output = Command::new("git")
            .args(["show", "-s", "--format=%B", &sha])
            .output()?;

        if !body_output.status.success() {
            return Err(anyhow!("git show failed"));
        }

        let body = String::from_utf8_lossy(&body_output.stdout);
        let has_keyword = subject.contains(&options.pull_request_keyword)
            || body.contains(&options.pull_request_keyword);

        if author == options.author_name && has_keyword {
            log_verbose(options.verbose, "-> matches keyword");
            commits.push(StackCommit { sha, subject });
        } else {
            if author != options.author_name {
                log_verbose(
                    options.verbose,
                    &format!(
                        "-> stopping: author '{}' does not match '{}'",
                        author, options.author_name
                    ),
                );
            } else if !has_keyword {
                log_verbose(
                    options.verbose,
                    &format!(
                        "-> stopping: keyword '{}' missing from commit",
                        options.pull_request_keyword
                    ),
                );
            }
            break;
        }
    }

    if commits.is_empty() {
        return Err(anyhow!(
            "No contiguous stack commits found for {} with keyword '{}'.",
            options.author_name,
            options.pull_request_keyword
        ));
    }

    Ok(commits)
}

fn select_stack_commit(commits: &[StackCommit]) -> Result<StackCommit> {
    if commits.is_empty() {
        return Err(anyhow!("No commits available for selection"));
    }

    println!("Select the PR commit to amend:");
    let items: Vec<String> = commits
        .iter()
        .map(|commit| format!("{} {}", abbreviate_sha(&commit.sha), commit.subject))
        .collect();

    let selection = Select::new().items(&items).default(0).interact()?;

    Ok(commits[selection].clone())
}

fn stage_changes() -> Result<()> {
    let status = Command::new("git")
        .args(["add", "-A"])
        .status()
        .context("Failed to stage changes")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("git add -A failed"))
    }
}

fn verify_acli_inputs(options: &FixupOptions) -> Result<()> {
    if options.acli_output_file.trim().is_empty() {
        return Err(anyhow!("ACLI_OUTPUT_FILE must be provided."));
    }
    if options.acli_output_schema.trim().is_empty() {
        return Err(anyhow!("ACLI_OUTPUT_SCHEMA must be provided."));
    }
    if options.acli_config_file.trim().is_empty() {
        return Err(anyhow!("ACLI_CONFIG_FILE must be provided."));
    }
    Ok(())
}

fn resolve_acli_message(default_message: &str) -> Result<String> {
    if !default_message.trim().is_empty() {
        return Ok(default_message.to_string());
    }

    loop {
        let input: String = Input::new()
            .with_prompt("Enter message for acli rovodev")
            .allow_empty(false)
            .interact_text()?;

        if !input.trim().is_empty() {
            return Ok(input);
        }
    }
}

fn request_commit_message(options: &FixupOptions, message: &str) -> Result<()> {
    let status = Command::new("acli")
        .arg("rovodev")
        .arg(message)
        .arg(format!("--output-file={}", options.acli_output_file))
        .arg(format!("--output-schema={}", options.acli_output_schema))
        .arg(format!("--config-file={}", options.acli_config_file))
        .status()
        .context("Failed to execute acli rovodev")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("acli rovodev exited with an error"))
    }
}

fn extract_commit_message(options: &FixupOptions) -> Result<Option<String>> {
    let path = Path::new(&options.acli_output_file);
    if !path.exists() {
        return Ok(None);
    }

    let data = fs::read_to_string(path).context("Failed to read acli output file")?;
    let parsed: Result<AcliOutput, _> = serde_json::from_str(&data);

    let message = match parsed {
        Ok(acli) => acli
            .commit_message
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty()),
        Err(_) => None,
    };

    let _ = fs::remove_file(path);
    Ok(message)
}

fn show_git_status() -> Result<()> {
    let output = Command::new("git")
        .args(["status", "-sb"])
        .output()
        .context("Failed to read git status")?;

    if !output.status.success() {
        return Err(anyhow!("git status failed"));
    }

    let status_text = String::from_utf8_lossy(&output.stdout);
    println!("{}", status_text.trim_end());
    Ok(())
}

fn create_fixup_commit(target_sha: &str) -> Result<()> {
    let status = Command::new("git")
        .arg("commit")
        .arg(format!("--fixup={}", target_sha))
        .status()
        .context("Failed to create fixup commit")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("git commit --fixup failed"))
    }
}

fn run_autosquash_rebase(target_sha: &str) -> Result<()> {
    let base = format!("{}^", target_sha);
    let status = Command::new("git")
        .env("GIT_SEQUENCE_EDITOR", ":")
        .args(["rebase", "-i", "--autosquash", &base])
        .status()
        .context("Failed to run git rebase")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("git rebase --autosquash failed"))
    }
}

fn print_suggested_message(message: &str) {
    eprintln!("Suggested commit message:");
    eprintln!("-------------------------");
    eprintln!("{}", message);
    eprintln!("-------------------------");
}

fn prompt_final_message(default_message: &str) -> Result<String> {
    let input: String = Input::new()
        .with_prompt("Press Enter to keep, or type a new commit message")
        .allow_empty(true)
        .interact_text()?;

    if input.trim().is_empty() {
        Ok(default_message.to_string())
    } else {
        Ok(input)
    }
}

fn submit_stack(message: &str) -> Result<()> {
    let status = Command::new("ag")
        .args(["submit", "-m", message, "--all"])
        .status()
        .context("Failed to execute ag submit")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("ag submit failed"))
    }
}

fn git_diff_has_changes(args: &[&str]) -> Result<bool> {
    let mut cmd = Command::new("git");
    cmd.arg("diff");
    for arg in args {
        cmd.arg(arg);
    }

    let status = cmd.status().context("git diff failed")?;
    match status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        Some(code) => Err(anyhow!("git diff exited with status {}", code)),
        None => Err(anyhow!("git diff terminated by signal")),
    }
}

fn abbreviate_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

fn log_verbose(enabled: bool, message: &str) {
    if enabled {
        println!("[VERBOSE] {}", message);
    }
}

fn config_path() -> Result<PathBuf> {
    match home_dir() {
        Some(dir) => Ok(dir.join(CONFIG_FILENAME)),
        None => Err(anyhow!("Unable to locate home directory for config file")),
    }
}

fn read_config() -> Result<Option<FixupOptions>> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    let parsed = parse_config(&contents)?;
    Ok(Some(parsed))
}

fn parse_config(contents: &str) -> Result<FixupOptions> {
    let mut options = FixupOptions::default();

    for (idx, raw_line) in contents.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| anyhow!("Invalid config line {}: {}", idx + 1, raw_line))?;
        apply_config_value(&mut options, key.trim(), value.trim())?;
    }

    Ok(options)
}

fn apply_config_value(options: &mut FixupOptions, key: &str, raw_value: &str) -> Result<()> {
    let value = raw_value.to_string();

    match key {
        "AUTHOR_NAME" => options.author_name = value,
        "STACK_PATTERN" => options.stack_pattern = value,
        "PULL_REQUEST_KEYWORD" => options.pull_request_keyword = value,
        "ACLI_MESSAGE" => options.acli_message = value,
        "ACLI_OUTPUT_FILE" => options.acli_output_file = value,
        "ACLI_OUTPUT_SCHEMA" => options.acli_output_schema = value,
        "ACLI_CONFIG_FILE" => options.acli_config_file = value,
        "STACK_LOG_LIMIT" => {
            let parsed: u32 = value
                .parse()
                .map_err(|_| anyhow!("STACK_LOG_LIMIT must be a positive integer"))?;
            options.stack_log_limit = parsed.max(1);
        }
        "VERBOSE" => {
            options.verbose =
                parse_bool(&value).ok_or_else(|| anyhow!("VERBOSE must be true/false"))?;
        }
        "DEFAULT_MESSAGE" => options.default_message = value,
        _ => {}
    }

    Ok(())
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn prompt_and_write_config(
    mode: ConfigInitMode,
    existing: Option<FixupOptions>,
) -> Result<FixupOptions> {
    let path = config_path()?;

    if matches!(mode, ConfigInitMode::Manual) && path.exists() {
        if let Some(current) = existing.clone() {
            let reconfigure = Confirm::new()
                .with_prompt(format!(
                    "Configuration already exists at {}. Reconfigure now?",
                    path.display()
                ))
                .default(true)
                .interact()?;

            if !reconfigure {
                ui::info("Keeping existing configuration");
                return Ok(current);
            }
        }
    }

    let defaults = existing.unwrap_or_default();
    let new_config = prompt_for_config(&defaults)?;

    ui::info(&format!("Writing configuration to {}", path.display()));
    write_config_file(&new_config)?;
    Ok(new_config)
}

fn prompt_for_config(defaults: &FixupOptions) -> Result<FixupOptions> {
    let author_name = prompt_string("Author name", &defaults.author_name)?;
    let stack_pattern = prompt_string("Stack branch regex", &defaults.stack_pattern)?;
    let pull_request_keyword = prompt_string(
        "Keyword identifying stack commits",
        &defaults.pull_request_keyword,
    )?;
    let acli_message = prompt_string("ACLI message", &defaults.acli_message)?;
    let acli_output_file = prompt_string("ACLI output file", &defaults.acli_output_file)?;
    let acli_output_schema = prompt_string("ACLI output schema", &defaults.acli_output_schema)?;
    let acli_config_file = prompt_string("ACLI config file", &defaults.acli_config_file)?;
    let stack_log_limit = prompt_positive_u32("Stack log limit", defaults.stack_log_limit)?;
    let verbose = Confirm::new()
        .with_prompt("Enable verbose logging?")
        .default(defaults.verbose)
        .interact()?;
    let default_message = prompt_string("Fallback commit message", &defaults.default_message)?;

    Ok(FixupOptions {
        author_name,
        stack_pattern,
        pull_request_keyword,
        acli_message,
        acli_output_file,
        acli_output_schema,
        acli_config_file,
        stack_log_limit,
        verbose,
        default_message,
    })
}

fn prompt_string(label: &str, default: &str) -> Result<String> {
    let prompt = format!("{} [{}]", label, default);
    let value: String = Input::new()
        .with_prompt(prompt)
        .allow_empty(true)
        .interact_text()?;

    if value.trim().is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value)
    }
}

fn prompt_positive_u32(label: &str, default: u32) -> Result<u32> {
    loop {
        let prompt = format!("{} [{}]", label, default);
        let value: String = Input::new()
            .with_prompt(prompt)
            .allow_empty(true)
            .interact_text()?;

        if value.trim().is_empty() {
            return Ok(default.max(1));
        }

        match value.trim().parse::<u32>() {
            Ok(number) if number > 0 => return Ok(number),
            _ => println!("Please enter a positive integer."),
        }
    }
}

fn write_config_file(config: &FixupOptions) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let mut content = String::new();
    content.push_str(&format!(
        "AUTHOR_NAME={}\n",
        sanitize_value(&config.author_name)
    ));
    content.push_str(&format!(
        "STACK_PATTERN={}\n",
        sanitize_value(&config.stack_pattern)
    ));
    content.push_str(&format!(
        "PULL_REQUEST_KEYWORD={}\n",
        sanitize_value(&config.pull_request_keyword)
    ));
    content.push_str(&format!(
        "ACLI_MESSAGE={}\n",
        sanitize_value(&config.acli_message)
    ));
    content.push_str(&format!(
        "ACLI_OUTPUT_FILE={}\n",
        sanitize_value(&config.acli_output_file)
    ));
    content.push_str(&format!(
        "ACLI_OUTPUT_SCHEMA={}\n",
        sanitize_value(&config.acli_output_schema)
    ));
    content.push_str(&format!(
        "ACLI_CONFIG_FILE={}\n",
        sanitize_value(&config.acli_config_file)
    ));
    content.push_str(&format!("STACK_LOG_LIMIT={}\n", config.stack_log_limit));
    content.push_str(&format!("VERBOSE={}\n", config.verbose));
    content.push_str(&format!(
        "DEFAULT_MESSAGE={}\n",
        sanitize_value(&config.default_message)
    ));

    fs::write(&path, content)
        .with_context(|| format!("Failed to write config file {}", path.display()))?;
    Ok(())
}

fn sanitize_value(value: &str) -> String {
    value.replace('\n', " ")
}

/// Set up the fixup resources directory and copy resource files
fn setup_fixup_resources() -> Result<()> {
    // Create the ../fixup/resources directory structure
    let fixup_resources_dir = Path::new("../fixup/resources");
    
    ui::info(&format!("Creating directory: {}", fixup_resources_dir.display()));
    fs::create_dir_all(fixup_resources_dir)
        .with_context(|| format!("Failed to create directory {}", fixup_resources_dir.display()))?;
    
    // Copy resource files from src/resources to ../fixup/resources
    let source_dir = Path::new("src/resources");
    let resource_files = ["acli_schema.json", "config.ag-stack.yml"];
    
    for file in &resource_files {
        let source_path = source_dir.join(file);
        let dest_path = fixup_resources_dir.join(file);
        
        if source_path.exists() {
            ui::info(&format!("Copying {} to {}", source_path.display(), dest_path.display()));
            fs::copy(&source_path, &dest_path)
                .with_context(|| format!("Failed to copy {} to {}", source_path.display(), dest_path.display()))?;
        } else {
            ui::info(&format!("Warning: Source file {} not found, skipping", source_path.display()));
        }
    }
    
    ui::success("Fixup resources directory set up successfully");
    Ok(())
}
