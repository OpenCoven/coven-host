use super::*;

fn parents(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/')
        .skip(1)
        .map(move |(i, _)| &path[..i])
}

impl Client {
    fn flush_pending(&self, db: &Connection, remote: &Peer) -> Result<Option<String>> {
        let pending = db
            .query_row(
                "SELECT path,expected,operation,bytes FROM pending WHERE slot=1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((path, expected, operation, data)) = pending else {
            return Ok(None);
        };
        validate_path(&path)?;
        match remote.upload(&path, &data, expected, Some(&operation)) {
            Ok(()) => {
                let tx = db.unchecked_transaction()?;
                record(&tx, &path, expected + 1, &digest(&data))?;
                tx.execute("DELETE FROM pending WHERE slot=1", [])?;
                tx.commit()?;
                Ok(None)
            }
            Err(e) if peer::is_write_rejected(&e) => {
                self.archive(&path, "rejected", &data)?;
                db.execute("DELETE FROM pending WHERE slot=1", [])?;
                Ok((!peer::is_precondition(&e)).then(|| format!("{path}: {e:#}")))
            }
            Err(e) => Err(e),
        }
    }

    fn save_conflict(
        &self,
        db: &Connection,
        folder: &Folder,
        remote: &Peer,
        path: &str,
        sha: &str,
        item: &Item,
    ) -> Result<()> {
        let local = read_matching(folder, path, sha)?;
        let distant = remote.download(path, item)?;
        let a = self.archive(path, "local", &local)?;
        let b = self.archive(path, "remote", &distant)?;
        db.execute(
            "INSERT OR REPLACE INTO conflicts VALUES (?,?,?,?)",
            params![path, a, b, item.revision],
        )?;
        Ok(())
    }

    pub(super) fn reconcile(&self, db: &Connection, folder: &Folder) -> Result<Vec<String>> {
        self.check_recovery(folder)?;
        let mut scan = folder.scan()?;
        let incoming = db
            .query_row(
                "SELECT path,revision,sha FROM incoming WHERE slot=1",
                [],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        if let Some((path, revision, sha)) = incoming {
            let tx = db.unchecked_transaction()?;
            if scan.files.get(&path).is_some_and(|(s, _)| s == &sha) {
                record(&tx, &path, revision, &sha)?;
            }
            tx.execute("DELETE FROM incoming WHERE slot=1", [])?;
            tx.commit()?;
        }

        let mut remote = self.remote(db)?;
        let conflicts = conflicts(db)?;
        check_history(
            &baseline(db)?,
            &conflicts,
            &self.inventory(db, &mut remote)?,
        )?;
        let mut errors: Vec<_> = self.flush_pending(db, &remote)?.into_iter().collect();
        let items = self.inventory(db, &mut remote)?;
        let base = baseline(db)?;
        check_history(&base, &conflicts, &items)?;
        let all: BTreeSet<_> = scan
            .files
            .keys()
            .chain(items.keys())
            .chain(base.keys())
            .cloned()
            .collect();
        let mut paths: Vec<_> = all.into_iter().collect();
        paths.sort_by_key(|p| {
            scan.files.get(p).map_or(0, |(_, s)| *s as i64)
                - items.get(p).map_or(0, |i| i.size as i64)
        });
        for path in &paths {
            let sha = scan.files.get(path).map(|(sha, _)| sha.clone());
            let sha = sha.as_deref();
            let item = items.get(path);
            let old = base.get(path);
            let old_sha = old.map(|(_, s)| s.as_str());
            if sha.is_none() && old.is_some() {
                errors.push(format!(
                    "{path}: deletion propagation unsupported; remote copy preserved"
                ));
                continue;
            }
            if (item.is_some()
                && (scan.directories.contains(path)
                    || parents(path).any(|p| scan.files.contains_key(p))))
                || (sha.is_some()
                    && (parents(path).any(|p| items.contains_key(p))
                        || items
                            .range(format!("{path}/")..)
                            .next()
                            .is_some_and(|(p, _)| p.starts_with(&format!("{path}/")))))
            {
                errors.push(format!(
                    "{path}: file/directory collision; move the unsynchronized local entry"
                ));
                continue;
            }
            if let Some(conflict) = conflicts.get(path) {
                if let (Some(sha), Some(item)) = (sha, item)
                    && (!conflict.local.ends_with(&format!("-{sha}"))
                        || !conflict.remote.ends_with(&format!("-{}", item.sha256))
                        || conflict.revision != item.revision)
                {
                    self.save_conflict(db, folder, &remote, path, sha, item)?;
                }
                continue;
            }
            if let (Some(sha), Some(item)) = (sha, item)
                && sha == item.sha256
            {
                if old.is_none_or(|(rev, s)| *rev != item.revision || s != sha) {
                    record(db, path, item.revision, sha)?;
                }
                continue;
            }
            match (sha, item) {
                (Some(sha), Some(i))
                    if Some(sha) != old_sha && Some(i.sha256.as_str()) != old_sha =>
                {
                    self.save_conflict(db, folder, &remote, path, sha, i)?;
                }
                (sha, Some(i)) if sha == old_sha => {
                    if !download_fits(&scan, path, i.size) {
                        errors.push(format!("{path}: download would exceed local Castle limits"));
                        continue;
                    }
                    let data = remote.download(path, i)?;
                    db.execute(
                        "INSERT INTO incoming VALUES (1,?,?,?)",
                        params![path, i.revision, i.sha256],
                    )?;
                    folder.replace(path, &data, sha)?;
                    let tx = db.unchecked_transaction()?;
                    record(&tx, path, i.revision, &i.sha256)?;
                    tx.execute("DELETE FROM incoming", [])?;
                    tx.commit()?;
                    scan.files
                        .insert(path.clone(), (i.sha256.clone(), data.len()));
                    scan.directories.extend(parents(path).map(str::to_owned));
                }
                (Some(sha), item) => {
                    let data = read_matching(folder, path, sha)?;
                    let expected = item.map_or(0, |i| i.revision);
                    db.execute(
                        "INSERT INTO pending VALUES (1,?,?,?,?)",
                        params![path, expected, Uuid::new_v4().simple().to_string(), data],
                    )?;
                    if let Some(error) = self.flush_pending(db, &remote)? {
                        errors.push(error);
                    }
                }
                _ => {}
            }
        }
        if db.query_row("SELECT EXISTS(SELECT 1 FROM conflicts)", [], |r| {
            r.get::<_, bool>(0)
        })? {
            errors.push("unresolved conflicts; see status and resolve".into());
        }
        if errors.is_empty() {
            let expected = baseline(db)?;
            let remote_items = self.inventory(db, &mut remote)?;
            let local = folder.scan()?;
            if expected.len() != remote_items.len()
                || expected.len() != local.files.len()
                || expected.iter().any(|(p, (rev, sha))| {
                    remote_items
                        .get(p)
                        .is_none_or(|i| i.revision != *rev || i.sha256 != *sha)
                        || local.files.get(p).is_none_or(|(s, _)| s != sha)
                })
            {
                errors.push("files changed during reconciliation; sync again".into());
            }
        }
        Ok(errors)
    }
}

fn read_matching(folder: &Folder, path: &str, sha: &str) -> Result<Vec<u8>> {
    let data = folder
        .read(path)?
        .context("local file disappeared during reconciliation")?;
    ensure!(
        digest(&data) == sha,
        "{path}: local file changed during reconciliation; sync again"
    );
    Ok(data)
}

fn download_fits(scan: &Scan, path: &str, size: usize) -> bool {
    let count = scan.files.len() + usize::from(!scan.files.contains_key(path));
    let new_dirs = parents(path)
        .filter(|p| !scan.directories.contains(*p))
        .count();
    let total = scan.files.values().map(|(_, s)| s).sum::<usize>()
        - scan.files.get(path).map_or(0, |(_, s)| *s)
        + size;
    count <= MAX_FILES
        && count + scan.directories.len() + new_dirs <= 16 * MAX_FILES
        && total <= MAX_TOTAL
}
