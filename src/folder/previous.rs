//! Keep the displaced file itself reachable. An editor can still own an open
//! handle to its inode long after the replacement's current hash was checked.
use super::{Dir, missing_or, native};
use crate::{MAX_FILES, MAX_TOTAL, validate_path};
use anyhow::{Context, Result, ensure};
use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};

const LOCK: &str = ".lock";

pub(super) fn archive_path(scratch: &Dir) -> Result<PathBuf> {
    let parent = scratch.path.parent().context("missing recovery parent")?;
    let hidden = scratch.path.file_name() == Some(OsStr::new(".coven-recovery"));
    Ok(parent.join(if hidden {
        ".coven-previous-versions"
    } else {
        "previous-versions"
    }))
}

pub(super) fn name(id: &str, expected: &str) -> Result<String> {
    ensure!(
        id.len() == 32
            && expected.len() == 64
            && id
                .bytes()
                .chain(expected.bytes())
                .all(|b| b.is_ascii_hexdigit()),
        "invalid previous version identity"
    );
    Ok(format!(".version-{id}-{expected}"))
}

fn walk(dir: &Dir, depth: usize, visit: &mut impl FnMut(&Dir, &OsStr) -> Result<()>) -> Result<()> {
    ensure!(
        depth <= 16,
        "unexpected previous version directory depth at {}",
        dir.path.display()
    );
    dir.each_name(|entry| {
        if depth == 0 && entry == OsStr::new(LOCK) {
            return Ok(());
        }
        if dir.is_dir(entry)? {
            walk(&dir.child(entry, false)?, depth + 1, visit)
        } else {
            visit(dir, entry)
        }
    })
}

pub(super) fn list(path: &Path) -> Result<Vec<PathBuf>> {
    let Some(dir) = missing_or(Dir::open(path, false))? else {
        return Ok(vec![]);
    };
    native::check_private(&dir.file)?;
    let mut files = vec![];
    walk(&dir, 0, &mut |dir, name| {
        files.push(dir.path.join(name));
        Ok(())
    })?;
    Ok(files)
}

pub(super) struct PreviousVersion {
    dir: Dir,
    name: String,
    _reservation: File,
}

impl PreviousVersion {
    pub fn reserve(
        parent: &Dir,
        leaf: &str,
        scratch: &Dir,
        stage: &str,
        path: &str,
        expected: &str,
    ) -> Result<Self> {
        let prepare = || -> Result<Self> {
            validate_path(path)?;
            let id = stage
                .strip_prefix("exchange-")
                .context("invalid exchange name")?;
            let name = name(id, expected)?;
            let archive = archive_path(scratch)?;
            let dir = Dir::open(&archive, true)?;
            ensure!(
                dir.id()?.0 == scratch.id()?.0,
                "previous versions must be on the destination volume"
            );
            native::check_private(&dir.file)?;
            let lock = dir.open_lock(LOCK.as_ref())?;
            native::check_private(&lock)?;
            fs2::FileExt::lock_exclusive(&lock)?;
            let mut bytes = native::fingerprint(&parent.open_file(leaf.as_ref())?)?.size;
            let mut count = 1;
            walk(&dir, 0, &mut |directory, entry| {
                count += 1;
                bytes =
                    bytes.saturating_add(native::fingerprint(&directory.open_file(entry)?)?.size);
                ensure!(
                    count <= MAX_FILES && bytes <= MAX_TOTAL as u64,
                    "previous versions are full; close editors, back up and move older versions out of {} before retrying (100 MiB or 1000 files)",
                    archive.display()
                );
                Ok(())
            })?;
            ensure!(
                bytes <= MAX_TOTAL as u64,
                "previous version exceeds archive limit"
            );
            let mut destination = dir;
            for part in path.split('/') {
                destination = destination.child(part.as_ref(), true)?;
            }
            Ok(Self {
                dir: destination,
                name,
                _reservation: lock,
            })
        };
        match prepare() {
            Ok(candidate) => Ok(candidate),
            Err(e) => {
                // Reservation always precedes publication. Only the incoming transfer exists here.
                scratch.remove(stage)?;
                Err(e)
            }
        }
    }

    pub fn promote(&self, scratch: &Dir, source: &str) -> Result<()> {
        scratch
            .move_new(source, &self.dir, &self.name)
            .with_context(|| {
                format!(
                    "cannot retain previous version at {}; inspect recovery in {}",
                    self.dir.path.join(&self.name).display(),
                    scratch.path.display()
                )
            })?;
        #[cfg(test)]
        if std::env::var_os(super::tests::CRASH_AT_PROMOTION).is_some() {
            std::process::exit(92);
        }
        Ok(())
    }
}
