//! Harness for exercising the native library against the Python reference CLI tests.
use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use coven_host::{CastleId, Client, folder, peer, peer_command_for};
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long, global = true, default_value = "demo")]
    castle: CastleId,
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
        #[arg(long, value_name = "ID=TOKEN_FILE")]
        castle_token: Vec<String>,
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
    let client = Client::new(&args.state).for_castle(&args.castle);
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
            castle_token,
        } => {
            let mut castles = vec![(args.castle.clone(), folder::token_from(&token_file)?)];
            for registration in castle_token {
                let (id, file) = registration
                    .split_once('=')
                    .ok_or_else(|| anyhow::anyhow!("expected ID=TOKEN_FILE"))?;
                castles.push((id.parse()?, folder::token_from(file.as_ref())?));
            }
            peer::serve_castles(&storage, port, castles)?;
        }
        Command::Get {
            path,
            url,
            token_file,
        } => std::io::stdout().write_all(&peer_command_for(
            &args.castle,
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
        } => std::io::stdout().write_all(&peer_command_for(
            &args.castle,
            "put",
            &url,
            &token_file,
            Some(&path),
            Some(&source),
        )?)?,
        Command::DemoResult { url, token_file } => std::io::stdout().write_all(
            &peer_command_for(&args.castle, "demo-result", &url, &token_file, None, None)?,
        )?,
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
