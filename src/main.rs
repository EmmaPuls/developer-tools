use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Subcommand)]
enum RovoCommands {
    /// Log into Rovo and launch rovodev
    Login,
}

mod commands;
mod utils;

use commands::{add, checkout, fixup, rovo, squash};

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
    /// Generate a script using AI from a prompt or interactive editor session
    Add {
        /// Describe the script to generate (skips editor if provided)
        #[arg(short, long)]
        message: Option<String>,
        /// Target scripting language
        #[arg(long, default_value = "bash", value_parser = ["bash", "js"])]
        lang: String,
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
        Commands::Add { message, lang } => {
            let lang = add::Lang::from_str(&lang)?;
            add::execute(message, lang)?;
        }
    }

    Ok(())
}
