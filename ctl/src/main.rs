use anyhow::Context;
use clap::Parser;
use common::Profile;
use common::ipc::{Ipc, IpcResponse};
use serde_json::to_string_pretty;
use std::collections::HashMap;
use std::io::{self, Write};

#[derive(Parser, Debug)]
#[command(name = "wl-clicker", about = "Control the wl-clicker-rs daemon")]
pub struct Args {
    #[command(subcommand)]
    pub command: Cli,

    /// Output in JSON format
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(Parser, Debug)]
pub enum Cli {
    /// List all available profiles
    Profiles,
    /// Show details of a specific profile
    Show {
        #[arg(help = "Name of the profile to show")]
        name: String,
    },
    /// Switch to a different profile
    Select {
        #[arg(help = "Name of the profile to activate")]
        name: String,
    },
    /// Show the currently active profile
    Current,
}

#[derive(Debug)]
pub struct KeyConfig {
    pub cps: u32,
    pub toggle: bool,
    pub jitter: u32,
}

fn format_profiles_pretty(profiles: &Vec<Profile>) -> String {
    let mut output = String::new();

    if profiles.is_empty() {
        return "No profiles found.".to_string();
    }

    for profile in profiles {
        output.push_str(&format_profile_pretty(profile));
        output.push_str("\n");
    }

    output.trim_end().to_string()
}

fn format_profile_pretty(profile: &Profile) -> String {
    let mut output = String::new();

    output.push_str(&format!("\x1b[34m{}\x1b[0m\n", profile.name));
    output.push_str(&format!(
        "  {:?} → {} CPS{}{}\n",
        profile.keys,
        profile.cps,
        if profile.toggle { " (toggle)" } else { "" },
        if profile.jitter > 0 {
            &format!(" ±{} jitter", profile.jitter)
        } else {
            ""
        }
    ));

    output
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut ipc = Ipc::connect().context("Failed to connect to IPC")?;
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();

    let response: IpcResponse = match args.command {
        Cli::Profiles => ipc.request_all_profiles()?,
        Cli::Show { ref name } => ipc.request_profile(name.to_owned())?,
        Cli::Select { ref name } => ipc.switch_profile(name.to_owned())?,
        Cli::Current => ipc.request_current_profile()?,
    };

    match response {
        IpcResponse::Ok => writeln!(stdout, "Operation successful")?,
        IpcResponse::Error(err) => writeln!(stderr, "Error: {}", err)?,
        IpcResponse::AllProfiles(profiles) => {
            if args.json {
                writeln!(stdout, "{}", to_string_pretty(&profiles)?)?;
            } else {
                writeln!(stdout, "{}", format_profiles_pretty(&profiles))?;
            }
        }
        IpcResponse::Profile(profile) => {
            if args.json {
                writeln!(stdout, "{}", to_string_pretty(&profile)?)?;
            } else {
                writeln!(stdout, "{}", format_profile_pretty(&profile))?;
            }
        }
    }

    stdout.flush()?;
    Ok(())
}
