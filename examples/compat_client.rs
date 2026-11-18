//! Harness for exercising the native library against the Python reference CLI tests.
use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use coven_host::{Client, folder, peer, peer_command};
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = ".")]
    state: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        root: PathBuf,
    },
    Connect {
        url: String,
        #[arg(long)]
        token_file: PathBuf,
    },
    Sync,
    Status,
    Watch,
    Resolve {
        path: String,
        #[arg(long)]
        local: bool,
        #[arg(long)]
        remote: bool,
        #[arg(long)]
        merged: Option<PathBuf>,
    },
    Serve {
        #[arg(long)]
        storage: PathBuf,
        #[arg(long)]
        port: u16,
        #[arg(long)]
        token_file: PathBuf,
    },
    Put {
        path: String,
        source: PathBuf,
        #[arg(long)]
        url: String,
        #[arg(long)]
        token_file: PathBuf,
    },
    Get {
        path: String,
        #[arg(long)]
        url: String,
        #[arg(long)]
        token_file: PathBuf,
    },
    DemoResult {
        #[arg(long)]
        url: String,
        #[arg(long)]
        token_file: PathBuf,
    },
}

fn run() -> Result<()> {
    let args = Args::parse();
    let client = Client::new(&args.state);
    match args.command {
        Command::Init { root } => client.initialize(&root)?,
        Command::Connect { url, token_file } => client.connect(&url, &token_file)?,
        Command::Sync => {
            let out = client.sync()?;
            let phase = out["phase"].as_str().unwrap_or_default();
            ensure!(
                phase == "synced",
                "{phase}: {}",
                out["error"].as_str().unwrap_or_default()
            );
            println!("{phase}");
        }
        Command::Watch => loop {
            let _ = client.sync()?;
            std::thread::sleep(std::time::Duration::from_secs(2));
        },
        Command::Status => {
            let s = client.status()?;
            println!(
                "Status: {}\nLast successful contact: {}\nPending local paths: {}\nConflicts: {}",
                s["phase"].as_str().unwrap_or_default(),
                s["contact"].as_str().unwrap_or("(none)"),
                s["pendingPaths"]
                    .as_u64()
                    .map_or("unknown".into(), |n| n.to_string()),
                s["conflicts"].as_array().map_or(0, Vec::len)
            );
            if let Some(p) = s["awaitingAcknowledgement"].as_str() {
                println!("Awaiting acknowledgement: {p}");
            }
            println!("{s}");
        }
        Command::Resolve {
            path,
            local,
            remote,
            merged,
        } => {
            ensure!(
                usize::from(local) + usize::from(remote) + usize::from(merged.is_some()) == 1,
                "choose exactly one resolution"
            );
            client.resolve(
                &path,
                if local {
                    "local"
                } else if remote {
                    "remote"
                } else {
                    "merged"
                },
                merged.as_deref(),
            )?;
        }
        Command::Serve {
            storage,
            port,
            token_file,
        } => peer::serve(&storage, port, folder::token_from(&token_file)?)?,
        Command::Get {
            path,
            url,
            token_file,
        } => std::io::stdout().write_all(&peer_command(
            "get",
            &url,
            &token_file,
            Some(&path),
            None,
        )?)?,
        Command::Put {
            path,
            source,
            url,
            token_file,
        } => std::io::stdout().write_all(&peer_command(
            "put",
            &url,
            &token_file,
            Some(&path),
            Some(&source),
        )?)?,
        Command::DemoResult { url, token_file } => std::io::stdout().write_all(&peer_command(
            "demo-result",
            &url,
            &token_file,
            None,
            None,
        )?)?,
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    if let Err(e) = run() {
        eprintln!("{e:#}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
