use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::folder::{Folder, Scan, private_dir, read_external, token_from};
use crate::peer::{self, Item, Peer};
use crate::{CastleId, MAX_FILE, MAX_FILES, MAX_TOTAL, digest, validate_path};

mod reconcile;

pub struct Client {
    castle: CastleId,
    base_state: PathBuf,
    profile: Option<PathBuf>,
    state: PathBuf,
}

type Baseline = BTreeMap<String, (i64, String)>;
struct Conflict {
    local: String,
    remote: String,
    revision: i64,
}

fn setting(db: &Connection, key: &str) -> Result<Option<String>> {
    Ok(db
        .query_row("SELECT value FROM settings WHERE key=?", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

fn set(db: &Connection, key: &str, value: &str) -> Result<()> {
    db.execute("INSERT OR REPLACE INTO settings VALUES (?,?)", [key, value])?;
    Ok(())
}

fn record(db: &Connection, path: &str, revision: i64, sha: &str) -> Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO baseline VALUES (?,?,?)",
        params![path, revision, sha],
    )?;
    Ok(())
}

fn baseline(db: &Connection) -> Result<Baseline> {
    let mut q = db.prepare("SELECT path,revision,sha FROM baseline")?;
    Ok(
        q.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
            .collect::<rusqlite::Result<_>>()?,
    )
}

fn conflicts(db: &Connection) -> Result<BTreeMap<String, Conflict>> {
    let mut q = db.prepare("SELECT path,local,remote,revision FROM conflicts ORDER BY path")?;
    Ok(q.query_map([], |r| {
        Ok((
            r.get(0)?,
            Conflict {
                local: r.get(1)?,
                remote: r.get(2)?,
                revision: r.get(3)?,
            },
        ))
    })?
    .collect::<rusqlite::Result<_>>()?)
}

fn phase(db: &Connection, value: &str, error: &str) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    set(&tx, "phase", value)?;
    set(&tx, "error", error)?;
    tx.commit()?;
    Ok(())
}

fn identity_path(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute() && !path.components().any(|c| matches!(c, Component::ParentDir)),
        "expected an absolute path without parent traversal"
    );
    match path.canonicalize() {
        Ok(p) => Ok(p),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(identity_path(
            path.parent().context("missing root directory")?,
        )?
        .join(path.file_name().context("missing path component")?)),
        Err(e) => Err(e.into()),
    }
}

fn separate(a: &Path, b: &Path) -> Result<()> {
    let (a, b) = (identity_path(a)?, identity_path(b)?);
    ensure!(
        !a.starts_with(&b) && !b.starts_with(&a),
        "state, token file and Coven root must be separate, non-nested paths"
    );
    Ok(())
}

impl Client {
    pub fn new(state: &Path) -> Self {
        Self {
            castle: CastleId::default(),
            base_state: state.into(),
            profile: None,
            state: state.to_owned(),
        }
    }

    pub fn for_profile(state: &Path, profile: &Path) -> Self {
        Self {
            profile: Some(profile.into()),
            ..Self::new(state)
        }
    }

    pub fn for_castle(mut self, id: &CastleId) -> Self {
        self.state = id.state_dir(&self.base_state);
        self.castle = id.clone();
        self
    }

    fn folder(&self, root: &Path) -> Folder {
        Folder::for_castle(root, &self.state, &self.castle)
    }

    fn owner(&self) -> Result<Option<String>> {
        self.profile
            .as_ref()
            .map(|profile| {
                let paths = (identity_path(profile)?, identity_path(&self.base_state)?);
                Ok(digest(&serde_json::to_vec(&paths)?))
            })
            .transpose()
    }

    fn open_lock(&self) -> Result<File> {
        private_dir(&self.state)?;
        let mut options = OpenOptions::new();
        options.create(true).append(true).read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        Ok(options.open(self.state.join("client.lock"))?)
    }

    fn lock(&self) -> Result<File> {
        let f = self.open_lock()?;
        FileExt::try_lock_exclusive(&f).context("sync is running; retry after it finishes")?;
        Ok(f)
    }

    fn db(&self, init: bool) -> Result<Connection> {
        let path = self.state.join("client.sqlite");
        if !init {
            ensure!(
                path.is_file(),
                "client is not initialized; run coven host init"
            );
        }
        let mut flags = rusqlite::OpenFlags::default() | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW;
        if !init {
            flags.remove(rusqlite::OpenFlags::SQLITE_OPEN_CREATE);
        }
        let db = Connection::open_with_flags(&path, flags)?;
        db.pragma_update(None, "synchronous", "FULL")?;
        if !init {
            self.root(&db)?;
            let tx = db.unchecked_transaction()?;
            tx.execute_batch(include_str!("client/deletion_schema.sql"))?;
            tx.commit()?;
        }
        Ok(db)
    }

    fn root(&self, db: &Connection) -> Result<PathBuf> {
        let root = PathBuf::from(setting(db, "root")?.context("client has no configured root")?);
        separate(&root, &self.base_state)?;
        if let Some(owner) = self.owner()? {
            self.folder(&root).verify_owner(&owner)?;
        }
        Ok(root)
    }

    pub fn initialize(&self, root: &Path) -> Result<()> {
        separate(root, &self.base_state)?;
        let _lock = self.lock()?;
        let mut db = self.db(true)?;
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("client/schema.sql"))?;
        let configured = setting(&tx, "root")?;
        if let Some(ref other) = configured {
            ensure!(
                identity_path(Path::new(&other))? == identity_path(root)?,
                "client state is already bound to another root"
            );
        }
        let folder = self.folder(root);
        if let Some(owner) = self.owner()? {
            folder.claim_owner(&owner, configured.is_some())?;
        }
        tx.execute_batch(include_str!("client/deletion_schema.sql"))?;
        folder.initialize()?;
        set(
            &tx,
            "root",
            root.to_str().context("root must be valid Unicode")?,
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn connect(&self, url: &str, token_file: &Path) -> Result<()> {
        let _lock = self.lock()?;
        let mut db = self.db(false)?;
        separate(&self.root(&db)?, token_file)?;
        peer::endpoint(url)?;
        token_from(token_file)?;
        let tx = db.transaction()?;
        set(&tx, "url", url)?;
        set(
            &tx,
            "token_path",
            token_file
                .to_str()
                .context("token path must be valid Unicode")?,
        )?;
        set(&tx, "phase", "needs attention")?;
        set(&tx, "error", "connection configured; run sync to reconcile")?;
        tx.commit()?;
        Ok(())
    }

    fn remote(&self, db: &Connection) -> Result<Peer> {
        let url = setting(db, "url")?.context("no remote configured; run connect")?;
        let token = setting(db, "token_path")?.context("no protected token file configured")?;
        separate(&self.root(db)?, Path::new(&token))?;
        Peer::for_castle(
            &self.castle,
            &url,
            token_from(Path::new(&token))?,
            setting(db, "host")?,
        )
    }

    fn inventory(&self, db: &Connection, remote: &mut Peer) -> Result<BTreeMap<String, Item>> {
        let items = remote.inventory()?;
        let tx = db.unchecked_transaction()?;
        set(
            &tx,
            "host",
            remote
                .identity
                .as_deref()
                .context("peer returned no identity")?,
        )?;
        set(
            &tx,
            "contact",
            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        )?;
        tx.commit()?;
        Ok(items)
    }

    fn archive(&self, path: &str, side: &str, data: &[u8]) -> Result<String> {
        let dir = self.state.join("recovery");
        private_dir(&dir)?;
        let name = format!("{}-{side}-{}", &digest(path.as_bytes())[..24], digest(data));
        let dest = dir.join(&name);
        if fs::symlink_metadata(&dest).is_ok() {
            ensure!(
                read_external(&dest, MAX_FILE)? == data,
                "recovery copy failed verification"
            );
            return Ok(name);
        }
        let mut total = data.len() as u64;
        for (i, entry) in fs::read_dir(&dir)?.enumerate() {
            total += fs::symlink_metadata(entry?.path())?.len();
            ensure!(
                i < 1999 && total <= MAX_TOTAL as u64,
                "recovery is full; back up and move resolved copies out of recovery"
            );
        }
        let mut tmp = tempfile::Builder::new()
            .prefix(".save-")
            .tempfile_in(&dir)?;
        tmp.write_all(data)?;
        tmp.as_file().sync_all()?;
        tmp.persist_noclobber(dest)?;
        #[cfg(unix)]
        File::open(&dir)?.sync_all()?;
        Ok(name)
    }

    fn retained(&self, folder: &Folder) -> Result<Vec<PathBuf>> {
        let mut paths = folder.recoveries()?;
        let dir = self.state.join("recovery");
        if dir.try_exists()? {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_str()
                    .and_then(|n| n.strip_prefix("exchange-"))
                    .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    paths.push(entry.path());
                }
            }
        }
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    fn check_recovery(&self, folder: &Folder) -> Result<()> {
        let copies = self.retained(folder)?;
        ensure!(
            copies.is_empty(),
            "interrupted or raced replacement; inspect recovery paths in status: {copies:?}"
        );
        Ok(())
    }

    pub fn sync(&self) -> Result<Value> {
        let lock = self.open_lock()?;
        FileExt::lock_exclusive(&lock)
            .context("could not acquire the client reconciliation lock")?;
        let db = self.db(false)?;
        let folder = self.folder(&self.root(&db)?);
        phase(&db, "syncing", "")?;
        let (state, error) = match self.reconcile(&db, &folder) {
            Ok(errors) if errors.is_empty() => ("synced", String::new()),
            Ok(errors) => ("needs attention", errors.join("; ")),
            Err(e) if peer::is_offline(&e) => ("offline", format!("{e:#}")),
            Err(e) => ("needs attention", format!("{e:#}")),
        };
        phase(&db, state, &error)?;
        Ok(json!({"phase":state,"error":error}))
    }

    pub fn resolve(&self, path: &str, choice: &str, merged: Option<&Path>) -> Result<()> {
        validate_path(path)?;
        let _lock = self.lock()?;
        let db = self.db(false)?;
        let folder = self.folder(&self.root(&db)?);
        self.check_recovery(&folder)?;
        let conflicts = conflicts(&db)?;
        let conflict = conflicts.get(path).context("no recorded conflict")?;
        let scan = folder.scan()?;
        let mut remote = self.remote(&db)?;
        let items = self.inventory(&db, &mut remote)?;
        check_history(&db, &baseline(&db)?, &conflicts, &items, &remote)?;
        if conflict.local.is_empty() || conflict.remote.is_empty() {
            let local = folder.read(path)?;
            let local_sha = local.as_deref().map(digest);
            ensure!(
                match &local_sha {
                    Some(sha) => conflict.local.ends_with(&format!("-{sha}")),
                    None => conflict.local.is_empty(),
                },
                "local conflict input changed; sync again before resolving"
            );
            let item = items.get(path);
            let distant = if let Some(item) = item {
                ensure!(
                    item.revision == conflict.revision
                        && conflict.remote.ends_with(&format!("-{}", item.sha256)),
                    "remote conflict input changed; sync again before resolving"
                );
                Some(remote.download(path, item)?)
            } else {
                ensure!(
                    conflict.remote.is_empty()
                        && remote.deleted.as_ref().and_then(|d| d.get(path))
                            == Some(&conflict.revision),
                    "remote conflict input changed; sync again before resolving"
                );
                None
            };
            let selected = match (choice, merged) {
                ("local", None) => local.clone(),
                ("remote", None) => distant,
                ("merged", Some(source)) => Some(read_external(source, MAX_FILE)?),
                _ => bail!("choose local, remote or a merged file"),
            };
            if selected != local {
                if let Some(data) = selected {
                    ensure!(
                        reconcile::download_fits(&scan, path, data.len()),
                        "resolution exceeds Castle storage limits"
                    );
                    folder.replace(path, &data, local_sha.as_deref())?;
                } else if let Some(sha) = local_sha {
                    folder.retain_deleted(path, &sha)?;
                }
            }
            let tx = db.unchecked_transaction()?;
            if let Some(item) = item {
                record(&tx, path, item.revision, &item.sha256)?;
            } else {
                tx.execute("DELETE FROM baseline WHERE path=?", [path])?;
                tx.execute(
                    "INSERT OR REPLACE INTO deleted_baseline VALUES (?,?)",
                    params![path, conflict.revision],
                )?;
            }
            tx.execute("DELETE FROM conflicts WHERE path=?", [path])?;
            set(&tx, "phase", "needs attention")?;
            set(
                &tx,
                "error",
                "resolution saved; run sync to confirm convergence",
            )?;
            tx.commit()?;
            return Ok(());
        }
        let item = items
            .get(path)
            .context("remote conflict input is missing")?;
        let local = folder
            .read(path)?
            .context("local conflict input is missing")?;
        let sha = digest(&local);
        ensure!(
            conflict.revision == item.revision
                && conflict.local.ends_with(&format!("-{sha}"))
                && conflict.remote.ends_with(&format!("-{}", item.sha256)),
            "conflict inputs changed; sync again before resolving"
        );
        let distant = remote.download(path, item)?;
        self.archive(path, "local", &local)?;
        self.archive(path, "remote", &distant)?;
        let selected = match (choice, merged) {
            ("local", None) => &local,
            ("remote", None) => &distant,
            ("merged", Some(source)) => &read_external(source, MAX_FILE)?,
            _ => bail!("choose local, remote or a merged file"),
        };
        if selected != &local {
            ensure!(
                scan.files.get(path).is_some_and(|(s, _)| s == &sha),
                "conflict input changed during scan"
            );
            ensure!(
                scan.files.values().map(|(_, size)| size).sum::<usize>() - local.len()
                    + selected.len()
                    <= MAX_TOTAL,
                "resolution exceeds Castle storage limits"
            );
            folder.replace(path, selected, Some(&sha))?;
        }
        let tx = db.unchecked_transaction()?;
        record(&tx, path, item.revision, &item.sha256)?;
        tx.execute("DELETE FROM conflicts WHERE path=?", [path])?;
        set(&tx, "phase", "needs attention")?;
        set(
            &tx,
            "error",
            "resolution saved; run sync to confirm convergence",
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn status(&self) -> Result<Value> {
        let lock = self.open_lock()?;
        let active = match FileExt::try_lock_shared(&lock) {
            Ok(()) => false,
            Err(e) if e.raw_os_error() == fs2::lock_contended_error().raw_os_error() => true,
            Err(e) => return Err(e.into()),
        };
        let db = self.db(false)?;
        let root = self.root(&db)?;
        let folder = self.folder(&root);
        let mut state = setting(&db, "phase")?.unwrap_or_else(|| "needs attention".into());
        let mut error = setting(&db, "error")?.unwrap_or_else(|| "run connect and sync".into());
        let pending: Option<String> = db
            .query_row(
                "SELECT path FROM pending UNION ALL SELECT path FROM pending_delete LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let base = baseline(&db)?;
        let count = match folder.scan() {
            Ok(scan) => {
                let mut changed: BTreeSet<_> = scan
                    .files
                    .iter()
                    .filter(|(p, (sha, _))| base.get(*p).map(|b| &b.1) != Some(sha))
                    .map(|(p, _)| p.clone())
                    .collect();
                let deleted: Vec<_> = base
                    .keys()
                    .filter(|p| !scan.files.contains_key(*p))
                    .cloned()
                    .collect();
                if !deleted.is_empty() {
                    state = "needs attention".into();
                    error = format!(
                        "missing local paths awaiting reconciliation: {}",
                        deleted.join(", ")
                    );
                }
                changed.extend(deleted);
                changed.extend(pending.iter().cloned());
                Some(changed.len())
            }
            Err(e) => {
                state = "needs attention".into();
                error = format!("{e:#}");
                None
            }
        };
        let recovery = self.state.join("recovery");
        let conflicts: Vec<_> = conflicts(&db)?
            .into_iter()
            .map(|(path, conflict)| {
                json!({
                    "path": path,
                    "local": (!conflict.local.is_empty()).then(|| recovery.join(&conflict.local)),
                    "localDeleted": conflict.local.is_empty(),
                    "remote": (!conflict.remote.is_empty()).then(|| recovery.join(&conflict.remote)),
                    "remoteDeleted": conflict.remote.is_empty(),
                    "revision": conflict.revision
                })
            })
            .collect();
        let retained = match self.retained(&folder) {
            Ok(paths) => paths,
            Err(e) => {
                state = "needs attention".into();
                error = format!("{e:#}");
                Vec::new()
            }
        };
        let previous_versions = match folder.previous_versions() {
            Ok(paths) => paths,
            Err(e) => {
                state = "needs attention".into();
                error = format!("{e:#}");
                Vec::new()
            }
        };
        if active {
            state = "syncing".into();
        } else if !retained.is_empty() {
            state = "needs attention".into();
            error = "inspect and move retained replacement copies before syncing".into();
        } else if state == "syncing" {
            state = "needs attention".into();
            error = "sync stopped before reconciliation; run sync".into();
        } else if state == "synced" && (count != Some(0) || !conflicts.is_empty()) {
            state = "needs attention".into();
            error = "local work pending; run sync".into();
        }
        let replacement_status = folder
            .replacement_status()
            .unwrap_or_else(|e| json!({"error":format!("{e:#}")}));
        Ok(json!({
            "castle": self.castle.as_str(),
            "root": root,
            "remote": setting(&db, "url")?,
            "contact": setting(&db, "contact")?,
            "phase": state,
            "error": error,
            "pendingPaths": count,
            "awaitingAcknowledgement": pending,
            "conflicts": conflicts,
            "recovery": recovery,
            "retained": retained,
            "replacementStatus": replacement_status,
            "previousVersions": previous_versions
        }))
    }
}

fn check_history(
    db: &Connection,
    base: &Baseline,
    conflicts: &BTreeMap<String, Conflict>,
    items: &BTreeMap<String, Item>,
    remote: &Peer,
) -> Result<()> {
    let verify = |path: &str, revision: i64, sha: &str| -> Result<()> {
        ensure!(
            remote
                .deleted
                .as_ref()
                .and_then(|d| d.get(path))
                .is_some_and(|v| *v > revision)
                || items.get(path).is_some_and(
                    |i| i.revision > revision || (i.revision == revision && i.sha256 == sha)
                ),
            "{path}: host history missing or rolled back; check persistent host storage"
        );
        Ok(())
    };
    let mut q = db.prepare("SELECT path,revision FROM deleted_baseline")?;
    let mut rows = q.query([])?;
    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        let rev: i64 = row.get(1)?;
        ensure!(
            remote
                .deleted
                .as_ref()
                .and_then(|d| d.get(&path))
                .is_some_and(|v| *v >= rev)
                || items.get(&path).is_some_and(|i| i.revision > rev),
            "{path}: deleted path history missing or rolled back"
        );
    }
    for (path, (revision, sha)) in base {
        verify(path, *revision, sha)?;
    }
    // A conflict is a newer durable observation even though the two sides have not converged.
    // Python and Rust both use the verified payload digest as the last part of the copy name.
    for (path, conflict) in conflicts {
        if conflict.remote.is_empty() {
            ensure!(
                remote
                    .deleted
                    .as_ref()
                    .and_then(|d| d.get(path))
                    .is_some_and(|v| *v >= conflict.revision)
                    || items
                        .get(path)
                        .is_some_and(|i| i.revision > conflict.revision),
                "{path}: deletion conflict history missing or rolled back"
            );
            continue;
        }
        let (_, sha) = conflict
            .remote
            .rsplit_once('-')
            .context("invalid remote conflict copy name")?;
        ensure!(
            sha.len() == 64 && sha.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid remote conflict digest"
        );
        verify(path, conflict.revision, sha)?;
    }
    Ok(())
}
