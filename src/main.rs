use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Subcommand)]
enum RovoCommands {
    /// Log into Rovo and launch rovodev
    Login,
}

mod commands;
mod utils;

use commands::{checkout, rovo, squash, duplicate};

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
    /// Undo last x commits and stash each one with its commit message
    Duplicate {
        /// Number of commits to undo and stash
        num_commits: u32,
        /// Enable debug logging
        #[arg(short, long)]
        verbose: bool,
        /// Skip stashing stage and go directly to branch creation
        #[arg(long)]
        no_stash: bool,
        /// Skip git hooks when creating commits
        #[arg(long)]
        no_verify: bool,
    },
    /// Rovo CLI wrapper with authentication
    Rovo {
        #[command(subcommand)]
        command: Option<RovoCommands>,
        /// Arguments to pass through to acli rovodev
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Checkout { branch, branch_flag, dry_run, git_args } => {
            checkout::execute(branch, branch_flag, dry_run, git_args)?;
        }
        Commands::Squash { count, message, dry_run, commit_args } => {
            squash::execute(count, message, dry_run, commit_args)?;
        }
        Commands::Duplicate { num_commits, verbose, no_stash, no_verify } => {
            duplicate::execute(num_commits, verbose, no_stash, no_verify)?;
        }
        Commands::Rovo { command, args } => {
            let rovo_cmd = command.map(|c| match c {
                RovoCommands::Login => rovo::RovoCommands::Login,
            });
            rovo::execute(rovo_cmd, args)?;
        }
    }

    Ok(())
}