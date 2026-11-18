//! Native single-owner Castle synchronization.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

pub const MAX_FILE: usize = 10 * 1024 * 1024;
pub const MAX_TOTAL: usize = 100 * 1024 * 1024;
pub const MAX_FILES: usize = 1000;

pub fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

pub fn excluded(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".", "~", "credentials", "secrets", "token", "id_"]
        .iter()
        .any(|p| lower.starts_with(p))
        || matches!(
            lower.as_str(),
            "node_modules" | "__pycache__" | "vendor" | "target" | "cache" | "caches"
        )
        || [
            ".key", ".pem", ".p12", ".pfx", ".ppk", ".kdbx", ".tmp", ".temp", ".swp", ".part",
            ".bak", ".sqlite", ".db", "~",
        ]
        .iter()
        .any(|s| lower.ends_with(s))
}

pub fn validate_path(path: &str) -> Result<()> {
    let parts: Vec<_> = path.split('/').collect();
    ensure!(
        path.len() <= 512
            && (2..=16).contains(&parts.len())
            && matches!(parts[0], "files" | "results"),
        "unsupported Castle path: {path:?}"
    );
    for part in &parts[1..] {
        let stem = part.split('.').next().unwrap_or_default();
        let device = matches!(stem, "con" | "prn" | "aux" | "nul")
            || ["com", "lpt"].iter().any(|prefix| {
                stem.strip_prefix(prefix)
                    .is_some_and(|s| matches!(s.as_bytes(), [b'1'..=b'9']))
            });
        ensure!(
            !part.is_empty()
                && part.len() <= 120
                && !excluded(part)
                && !device
                && !part.ends_with('.')
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_-.".contains(&c)),
            "unsupported Castle path: {path:?}"
        );
    }
    Ok(())
}

mod client;
pub mod folder;
pub mod peer;
pub use client::Client;

use std::path::Path;

pub fn peer_command(
    command: &str,
    url: &str,
    token_file: &Path,
    path: Option<&str>,
    source: Option<&Path>,
) -> Result<Vec<u8>> {
    use anyhow::Context;
    let path = if command == "demo-result" {
        "results/hello-from-host.md"
    } else {
        path.context("a Castle path is required")?
    };
    validate_path(path)?;
    let mut peer = peer::Peer::new(url, folder::token_from(token_file)?, None)?;
    let inventory = peer.inventory()?;
    let current = inventory.get(path);
    if command == "get" {
        return peer.download(path, current.context("file not on host")?);
    }
    let data = match command {
        "put" => folder::read_external(source.context("source file is required")?, MAX_FILE)?,
        "demo-result" => b"# Hello from Coven Host\n\nCastle: `demo`\n\nDemo message: this artifact came from the host, not a live agent.\n".to_vec(),
        _ => anyhow::bail!("unknown peer operation"),
    };
    if let Some(item) = current
        && peer.download(path, item)? == data
    {
        return Ok(format!("{path}: already current (revision {})\n", item.revision).into_bytes());
    }
    let expected = current.map_or(0, |item| item.revision);
    peer.upload(path, &data, expected, None)?;
    Ok(format!(
        "{path}: revision {}, SHA-256 {}\n",
        expected + 1,
        digest(&data)
    )
    .into_bytes())
}
