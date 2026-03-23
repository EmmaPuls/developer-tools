use crate::utils::ui;
use anyhow::Result;
use std::process::Command;

// Define RovoCommands here to avoid import issues
#[derive(Debug)]
pub enum RovoCommands {
    Login,
}

/// Execute the rovo command - equivalent to rovo.sh
pub fn execute(command: Option<RovoCommands>, args: Vec<String>) -> Result<()> {
    println!("Rovo CLI Wrapper v1.0.0");

    match command {
        Some(RovoCommands::Login) => {
            handle_login()?;
        }
        None => {
            // Default behavior: pass through to acli rovodev
            if command_exists("acli") {
                let status = Command::new("acli").arg("rovodev").args(&args).status()?;

                if !status.success() {
                    std::process::exit(status.code().unwrap_or(1));
                }
            } else {
                ui::error("acli command not found. Please install acli first.");
                ui::info("For more information, visit: https://developer.atlassian.com/cloud/acli/guides/introduction/");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn handle_login() -> Result<()> {
    ui::info("Logging into Rovo...");

    let login_status = Command::new("acli")
        .args(["rovodev", "auth", "login"])
        .status()?;

    if login_status.success() {
        ui::info("Launching rovodev...");
        let run_status = Command::new("acli").args(["rovodev", "run"]).status()?;

        if !run_status.success() {
            std::process::exit(run_status.code().unwrap_or(1));
        }
    } else {
        std::process::exit(login_status.code().unwrap_or(1));
    }

    Ok(())
}

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
