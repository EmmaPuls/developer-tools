use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Subcommand)]
enum RovoCommands {
    /// Log into Rovo and launch rovodev
    Login,
}

mod commands;
mod utils;

use commands::{checkout, fixup, rovo, skills, squash};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dt")]
#[command(about = "Git and development workflow tools")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Smart git checkout that handles remote/local branches automatically
    Checkout {
        /// Branch name to checkout or create
        branch: String,
        /// Create new branch (equivalent to git checkout -b)
        #[arg(short, long)]
        branch_flag: bool,
        /// Show what would happen without executing
        #[arg(long)]
        dry_run: bool,
        /// Additional git checkout options
        #[arg(last = true)]
        git_args: Vec<String>,
    },
    /// Interactive git squash with preview and confirmation
    Squash {
        /// Number of commits to squash
        count: u32,
        /// Commit message for squashed commit
        #[arg(short, long)]
        message: Option<String>,
        /// Show what would happen without executing
        #[arg(long)]
        dry_run: bool,
        /// Additional git commit options
        #[arg(last = true)]
        commit_args: Vec<String>,
    },
    /// Rovo CLI wrapper with authentication
    Rovo {
        #[command(subcommand)]
        command: Option<RovoCommands>,
        /// Arguments to pass through to acli rovodev
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Manage the stack fixup workflow
    Fixup {
        #[command(subcommand)]
        command: Option<FixupSubcommand>,
        /// Enable verbose logging
        #[arg(long)]
        verbose: bool,
    },
    /// List, install, enable and disable Claude skills
    Skills {
        #[command(subcommand)]
        command: Option<SkillsCommands>,
    },
}

#[derive(Subcommand)]
enum SkillsCommands {
    /// List all Claude skills, where they live and whether they're on
    ///
    /// Columns: STATE is on/off, MODEL is whether Claude can invoke it itself, SOURCE is where it comes from, PATH is its folder.
    List {
        /// Also look for project skills in this repo (repeatable; defaults to the current repo)
        #[arg(long = "repo")]
        repos: Vec<PathBuf>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Enable global skills (removes the override)
    Enable {
        /// Skill names
        #[arg(required = true)]
        names: Vec<String>,
        /// Show what would happen without executing
        #[arg(long)]
        dry_run: bool,
    },
    /// Disable global skills (sets skillOverrides to "off")
    Disable {
        /// Skill names
        #[arg(required = true)]
        names: Vec<String>,
        /// Show what would happen without executing
        #[arg(long)]
        dry_run: bool,
    },
    /// Install a skill from a local directory, git URL or owner/repo
    Install {
        /// Local path, git URL or owner/repo
        source: String,
        /// Subdirectory of the source containing the skill(s)
        #[arg(long)]
        path: Option<String>,
        /// Install under this name (single skill only)
        #[arg(long)]
        name: Option<String>,
        /// Install enabled
        #[arg(long)]
        on: bool,
        /// Install disabled
        #[arg(long)]
        off: bool,
        /// Overwrite an existing skill
        #[arg(long)]
        force: bool,
        /// Show what would happen without executing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
enum FixupSubcommand {
    /// Initialize or update the fixup configuration file
    Init,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Checkout {
            branch,
            branch_flag,
            dry_run,
            git_args,
        } => {
            checkout::execute(branch, branch_flag, dry_run, git_args)?;
        }
        Commands::Squash {
            count,
            message,
            dry_run,
            commit_args,
        } => {
            squash::execute(count, message, dry_run, commit_args)?;
        }
        Commands::Rovo { command, args } => {
            let rovo_cmd = command.map(|c| match c {
                RovoCommands::Login => rovo::RovoCommands::Login,
            });
            rovo::execute(rovo_cmd, args)?;
        }
        Commands::Fixup { command, verbose } => match command {
            Some(FixupSubcommand::Init) => fixup::run_init(verbose)?,
            None => fixup::run_fixup(verbose)?,
        },
        Commands::Skills { command } => match command {
            None => skills::run_list(Vec::new(), false)?,
            Some(SkillsCommands::List { repos, json }) => skills::run_list(repos, json)?,
            Some(SkillsCommands::Enable { names, dry_run }) => {
                skills::run_toggle(names, true, dry_run)?
            }
            Some(SkillsCommands::Disable { names, dry_run }) => {
                skills::run_toggle(names, false, dry_run)?
            }
            Some(SkillsCommands::Install {
                source,
                path,
                name,
                on,
                off,
                force,
                dry_run,
            }) => skills::run_install(skills::InstallOptions {
                source,
                path,
                name,
                on,
                off,
                force,
                dry_run,
            })?,
        },
    }

    Ok(())
}
