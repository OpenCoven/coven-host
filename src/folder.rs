//! Read and publish files through a pinned parent directory.
use crate::{MAX_FILE, MAX_FILES, MAX_TOTAL, digest, excluded, validate_path};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[cfg(unix)]
#[path = "folder/unix.rs"]
mod native;
#[cfg(windows)]
#[path = "folder/windows.rs"]
mod native;
use native::Dir;
mod previous;

#[derive(Debug, Default)]
pub struct Scan {
    pub files: BTreeMap<String, (String, usize)>,
    pub directories: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct Folder {
    root: PathBuf,
    state: PathBuf,
}

fn missing_or<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Ok(value) => Ok(Some(value)),
        Err(e)
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

fn read_at(parent: &Dir, name: &OsStr, limit: usize, private: bool) -> Result<Vec<u8>> {
    let file = parent.open_file(name)?;
    let before = native::fingerprint(&file)?;
    if private {
        native::check_private(&file)?;
    }
    ensure!(
        before.size <= limit as u64,
        "{name:?} exceeds {limit} byte limit"
    );
    let mut data = Vec::with_capacity(before.size as usize);
    (&file)
        .take(limit.saturating_add(1) as u64)
        .read_to_end(&mut data)?;
    ensure!(
        data.len() <= limit
            && data.len() as u64 == before.size
            && before == native::fingerprint(&file)?,
        "{name:?} changed while reading"
    );
    let current = parent.open_file(name)?;
    ensure!(
        before == native::fingerprint(&current)?,
        "{name:?} changed while reading"
    );
    Ok(data)
}

fn write_new(parent: &Dir, name: &str, data: &[u8]) -> Result<()> {
    let mut file = parent.create(name.as_ref())?;
    file.write_all(data)?;
    file.sync_all()?;
    parent.sync()
}

fn seed(parent: &Dir, name: &str, data: &[u8]) -> Result<()> {
    match write_new(parent, name, data) {
        Err(e)
            if e.downcast_ref::<io::Error>()
                .is_some_and(|e| e.kind() == io::ErrorKind::AlreadyExists) =>
        {
            // A pre-existing seed is owner content. Verify that its name and type are safe.
            native::fingerprint(&parent.open_file(name.as_ref())?)?;
            Ok(())
        }
        other => other,
    }
}

pub fn private_dir(path: &Path) -> Result<()> {
    let dir = Dir::open(path, true)?;
    native::check_private(&dir.file)
}

pub fn read_external(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let parent = Dir::open(path.parent().context("missing file parent")?, false)?;
    read_at(
        &parent,
        path.file_name().context("missing file name")?,
        limit,
        false,
    )
}

pub fn token_from(path: &Path) -> Result<String> {
    let parent = Dir::open(path.parent().context("missing token parent")?, false)?;
    let data = read_at(
        &parent,
        path.file_name().context("missing token name")?,
        256,
        true,
    )?;
    ensure!(data.is_ascii(), "invalid token file format");
    let token = std::str::from_utf8(&data)?.trim_ascii();
    ensure!(
        (43..=128).contains(&token.len())
            && token
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')),
        "token file must contain one URL-safe token of 43–128 characters"
    );
    Ok(token.to_owned())
}

impl Folder {
    pub fn new(root: &Path, state: &Path) -> Self {
        Self {
            root: root.into(),
            state: state.into(),
        }
    }

    fn castle(&self, create: bool) -> Result<Dir> {
        Dir::open(&self.root, create)?
            .child("Castles".as_ref(), create)?
            .child("demo".as_ref(), create)
    }

    fn opened(&self) -> Result<Dir> {
        let castle = self.castle(false).context("root unavailable or unsafe")?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&read_at(&castle, "castle.json".as_ref(), 4096, false)?)?;
        ensure!(
            manifest["version"].as_u64() == Some(1) && manifest["id"].as_str() == Some("demo"),
            "wrong Castle manifest; restore the original demo folder"
        );
        Ok(castle)
    }

    pub fn initialize(&self) -> Result<()> {
        let castle = self.castle(true)?;
        for area in ["files", "results"] {
            castle.child(area.as_ref(), true)?;
        }
        seed(
            &castle,
            "castle.json",
            b"{\n  \"version\": 1,\n  \"id\": \"demo\",\n  \"displayName\": \"Demo Castle\"\n}\n",
        )?;
        seed(&Dir::open(&self.root, false)?, "README.md", b"# Coven\n\nOnly Castles/demo/files and results sync. Missing files require resolution. Run coven host status for pending work and recovery paths.\n")?;
        self.opened()?;
        Ok(())
    }

    pub fn scan(&self) -> Result<Scan> {
        fn walk(
            dir: &Dir,
            prefix: &str,
            scan: &mut Scan,
            count: &mut usize,
            total: &mut usize,
        ) -> Result<()> {
            dir.each_name(|name| {
                let name = name.to_str().context("unsupported Castle filename")?;
                if excluded(name) {
                    return Ok(());
                }
                let path = format!("{prefix}/{name}");
                validate_path(&path)?;
                *count += 1;
                ensure!(
                    *count <= 16 * MAX_FILES,
                    "Castle exceeds 16000 file and directory entries"
                );
                if dir.is_dir(name.as_ref())? {
                    scan.directories.insert(path.clone());
                    walk(&dir.child(name.as_ref(), false)?, &path, scan, count, total)?;
                } else {
                    let data = read_at(dir, name.as_ref(), MAX_FILE, false)?;
                    *total += data.len();
                    ensure!(
                        *total <= MAX_TOTAL && scan.files.len() < MAX_FILES,
                        "local Castle exceeds 100 MiB or 1000 files"
                    );
                    scan.files.insert(path, (digest(&data), data.len()));
                }
                Ok(())
            })
        }
        let castle = self.opened()?;
        let mut scan = Scan::default();
        let (mut count, mut total) = (0, 0);
        for area in ["files", "results"] {
            walk(
                &castle
                    .child(area.as_ref(), false)
                    .context("root unavailable or unsafe; mount or restore it")?,
                area,
                &mut scan,
                &mut count,
                &mut total,
            )?;
        }
        Ok(scan)
    }

    fn parent(&self, relative: &str, create: bool) -> Result<Dir> {
        let mut parts = relative.split('/');
        let mut dir = self
            .opened()?
            .child(parts.next().context("missing area")?.as_ref(), false)?;
        for part in parts {
            dir = dir.child(part.as_ref(), create)?;
        }
        Ok(dir)
    }

    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>> {
        validate_path(path)?;
        let (relative, name) = path.rsplit_once('/').context("missing area")?;
        // A missing root or scope is an unavailable Castle, not a missing document.
        self.opened()?
            .child(path.split('/').next().unwrap().as_ref(), false)?;
        let Some(parent) = missing_or(self.parent(relative, false))? else {
            return Ok(None);
        };
        missing_or(read_at(&parent, name.as_ref(), MAX_FILE, false))
    }

    fn scratch(&self, target: &Dir) -> Result<Dir> {
        let state = Dir::open(&self.state, true)?;
        native::check_private(&state.file)?;
        let scratch = if state.id()?.0 == target.id()?.0 {
            state.child("recovery".as_ref(), true)?
        } else {
            Dir::open(&self.root, false)?.child(".coven-recovery".as_ref(), true)?
        };
        native::check_private(&scratch.file)?;
        ensure!(
            scratch.id()?.0 == target.id()?.0,
            "cannot stage on the destination volume"
        );
        Ok(scratch)
    }

    pub fn replace(&self, path: &str, data: &[u8], expected: Option<&str>) -> Result<()> {
        validate_path(path)?;
        ensure!(data.len() <= MAX_FILE, "download exceeds file size limit");
        let (relative, name) = path.rsplit_once('/').context("missing area")?;
        let parent = self.parent(relative, true)?;
        let scratch = self.scratch(&parent)?;
        let id = Uuid::new_v4().simple().to_string();
        let stage = format!(
            "{}-{id}",
            if expected.is_some() {
                "exchange"
            } else {
                "stage"
            }
        );
        write_new(&scratch, &stage, data)?;
        let verify = || -> Result<()> {
            ensure!(
                self.parent(relative, false)?.id()? == parent.id()?,
                "{path}: parent moved during transfer; retry"
            );
            let previous =
                missing_or(read_at(&parent, name.as_ref(), MAX_FILE, false))?.map(|b| digest(&b));
            ensure!(
                previous.as_deref() == expected,
                "{path}: local file changed during transfer; retry"
            );
            Ok(())
        };
        if let Err(e) = verify() {
            scratch.remove(&stage)?;
            return Err(e);
        }
        native::install(&parent, name, &scratch, &stage, expected, path, data)?;
        ensure!(
            self.parent(relative, false)?.id()? == parent.id()?,
            "{path}: parent moved during replacement; inspect the moved directory"
        );
        Ok(())
    }

    pub(crate) fn retain_deleted(&self, path: &str, expected: &str) -> Result<()> {
        validate_path(path)?;
        let (relative, leaf) = path.rsplit_once('/').context("missing area")?;
        let parent = self.parent(relative, false)?;
        let scratch = self.scratch(&parent)?;
        let id = Uuid::new_v4().simple().to_string();
        let retained = previous::PreviousVersion::reserve_for_move(
            &parent, leaf, &scratch, &id, path, expected,
        )?;
        ensure!(
            self.parent(relative, false)?.id()? == parent.id()?
                && digest(&read_at(&parent, leaf.as_ref(), MAX_FILE, false)?) == expected,
            "{path}: file or parent changed during deletion; retry"
        );
        retained.retain_deleted(&parent, leaf, expected)?;
        ensure!(
            self.parent(relative, false)?.id()? == parent.id()?,
            "{path}: parent moved during deletion; inspect previous versions"
        );
        Ok(())
    }

    pub fn previous_versions(&self) -> Result<Vec<PathBuf>> {
        let mut files = previous::list(&self.state.join("previous-versions"))?;
        // Status also reports the root availability error separately. State-side
        // versions remain available while a removable or unsafe root is offline.
        if Dir::open(&self.root, false).is_ok() {
            files.extend(previous::list(&self.root.join(".coven-previous-versions"))?);
        }
        files.sort();
        Ok(files)
    }

    pub fn replacement_status(&self) -> Result<serde_json::Value> {
        #[cfg(windows)]
        {
            let mut rows = Vec::new();
            for path in self.recoveries()? {
                let Some(name) = path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .filter(|n| n.starts_with("replace-"))
                else {
                    continue;
                };
                let result = Dir::open(path.parent().context("missing journal parent")?, false)
                    .and_then(|dir| native::inspect(self, &dir, name));
                rows.push(
                    result.unwrap_or_else(
                        |e| serde_json::json!({"journal":path,"error":e.to_string()}),
                    ),
                );
            }
            Ok(rows.into())
        }
        #[cfg(unix)]
        {
            Ok(serde_json::json!([]))
        }
    }

    pub fn recoveries(&self) -> Result<Vec<PathBuf>> {
        let mut found = Vec::new();
        for directory in [
            self.state.join("recovery"),
            self.root.join(".coven-recovery"),
        ] {
            let Some(dir) = missing_or(Dir::open(&directory, false))? else {
                continue;
            };
            native::check_private(&dir.file)?;
            dir.each_name(|name| {
                if let Some(name) = name.to_str().filter(|name| is_recovery_name(name)) {
                    found.push(directory.join(name));
                }
                Ok(())
            })?;
        }
        found.sort();
        found.dedup();
        Ok(found)
    }
}

fn is_recovery_name(name: &str) -> bool {
    ["exchange-", "stage-", "replace-", "backup-", "prepare-"]
        .iter()
        .any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fixture() -> (tempfile::TempDir, Folder, Dir, Dir) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let folder = Folder::new(&root.join("Coven"), &root.join("state"));
        folder.initialize().unwrap();
        let parent = folder.parent("files", false).unwrap();
        let scratch = folder.scratch(&parent).unwrap();
        (tmp, folder, parent, scratch)
    }

    #[test]
    fn edit_after_hash_check_retains_the_exact_displaced_bytes() {
        let (_tmp, folder, parent, scratch) = fixture();
        write_new(&parent, "item", b"baseline").unwrap();
        let expected = digest(&read_at(&parent, "item".as_ref(), MAX_FILE, false).unwrap());
        let stage = "exchange-00000000000000000000000000000001";
        write_new(&scratch, stage, b"remote").unwrap();
        fs::write(parent.path.join("item"), b"edit in the swap window").unwrap();
        native::install(
            &parent,
            "item",
            &scratch,
            stage,
            Some(&expected),
            "files/item",
            b"remote",
        )
        .unwrap_err();
        assert_eq!(folder.read("files/item").unwrap().unwrap(), b"remote");
        let retained = folder.recoveries().unwrap();
        assert!(
            retained
                .iter()
                .any(|p| fs::read(p).unwrap() == b"edit in the swap window")
        );
        #[cfg(windows)]
        {
            let row = &folder.replacement_status().unwrap()[0];
            assert_eq!(row["backup"]["sha256"], digest(b"edit in the swap window"));
            assert_eq!(row["destination"]["sha256"], digest(b"remote"));
        }
    }

    #[test]
    fn create_preserves_entry_appearing_after_absence_check() {
        let (_tmp, folder, parent, scratch) = fixture();
        assert!(folder.read("files/item").unwrap().is_none());
        let stage = "stage-00000000000000000000000000000002";
        write_new(&scratch, stage, b"remote").unwrap();
        write_new(&parent, "item", b"racing creation").unwrap();
        assert!(
            native::install(
                &parent,
                "item",
                &scratch,
                stage,
                None,
                "files/item",
                b"remote"
            )
            .is_err()
        );
        assert_eq!(
            fs::read(parent.path.join("item")).unwrap(),
            b"racing creation"
        );
    }

    const CRASH_CHILD: &str = "COVEN_HOST_TEST_EXIT_AFTER_NATIVE_REPLACEMENT";

    pub(super) fn exit_at_publication() {
        if std::env::var_os(CRASH_CHILD).is_some() {
            std::process::exit(91);
        }
    }

    #[test]
    fn restart_reports_displaced_bytes_after_process_exit_at_publication() {
        if let Some(path) = std::env::var_os(CRASH_CHILD) {
            let dir = PathBuf::from(path);
            let child = Folder::new(&dir.join("Coven"), &dir.join("state"));
            child
                .replace("files/item", b"downloaded", Some(&digest(b"offline edit")))
                .unwrap();
            panic!("replacement returned before the injected process exit");
        }
        let (tmp, folder, parent, scratch) = fixture();
        write_new(&parent, "item", b"offline edit").unwrap();
        drop(parent);
        drop(scratch);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "folder::tests::restart_reports_displaced_bytes_after_process_exit_at_publication",
            ])
            .env(CRASH_CHILD, tmp.path().canonicalize().unwrap())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(91));
        assert_eq!(folder.read("files/item").unwrap().unwrap(), b"downloaded");
        assert!(
            folder
                .recoveries()
                .unwrap()
                .iter()
                .any(|p| fs::read(p).unwrap() == b"offline edit")
        );
        #[cfg(windows)]
        {
            let report = folder.replacement_status().unwrap();
            assert_eq!(report[0]["backup"]["sha256"], digest(b"offline edit"));
            assert_eq!(report[0]["destination"]["sha256"], digest(b"downloaded"));
            assert_eq!(report[0]["staging"]["missing"], true);
        }
    }

    pub(super) const CRASH_AT_PROMOTION: &str = "COVEN_HOST_TEST_EXIT_AFTER_VERSION_PROMOTION";

    #[test]
    fn restart_after_promotion_preserves_version_and_journal_evidence() {
        if let Some(path) = std::env::var_os(CRASH_AT_PROMOTION) {
            let root = PathBuf::from(path);
            let folder = Folder::new(&root.join("Coven"), &root.join("state"));
            folder
                .replace("files/item", b"downloaded", Some(&digest(b"offline edit")))
                .unwrap();
            panic!("expected exit after promotion");
        }
        let (tmp, folder, parent, scratch) = fixture();
        write_new(&parent, "item", b"offline edit").unwrap();
        drop(parent);
        drop(scratch);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "folder::tests::restart_after_promotion_preserves_version_and_journal_evidence",
            ])
            .env(CRASH_AT_PROMOTION, tmp.path().canonicalize().unwrap())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(92));
        assert_eq!(folder.read("files/item").unwrap().unwrap(), b"downloaded");
        let saved = folder.previous_versions().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(fs::read(&saved[0]).unwrap(), b"offline edit");
        #[cfg(windows)]
        {
            assert_eq!(folder.recoveries().unwrap().len(), 1);
            let row = &folder.replacement_status().unwrap()[0];
            assert_eq!(row["previous"]["sha256"], digest(b"offline edit"));
            assert_eq!(row["backup"]["missing"], true);
            assert_eq!(row["destination"]["sha256"], digest(b"downloaded"));
        }
    }

    #[cfg(unix)]
    #[test]
    fn publish_rejects_a_parent_replaced_after_the_input_check() {
        let (_tmp, folder, parent, scratch) = fixture();
        let stage = "exchange-00000000000000000000000000000004";
        write_new(&parent, "item", b"baseline").unwrap();
        write_new(&scratch, stage, b"incoming").unwrap();
        let moved = parent.path.with_file_name("moved");
        fs::rename(&parent.path, &moved).unwrap();
        fs::create_dir(&parent.path).unwrap();
        fs::write(parent.path.join("item"), b"new owner data").unwrap();
        assert!(
            native::install(
                &parent,
                "item",
                &scratch,
                stage,
                Some(&digest(b"baseline")),
                "files/item",
                b"incoming"
            )
            .is_err()
        );
        assert_eq!(
            folder.read("files/item").unwrap().unwrap(),
            b"new owner data"
        );
        assert_eq!(fs::read(moved.join("item")).unwrap(), b"baseline");
    }

    #[cfg(windows)]
    #[test]
    fn sharing_violation_preserves_original_and_junction_cannot_redirect_writes() {
        use std::os::windows::{fs::OpenOptionsExt, process::CommandExt};
        let (_tmp, folder, parent, _scratch) = fixture();
        write_new(&parent, "item", b"original").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(parent.path.join("item"))
            .unwrap();
        assert!(
            folder
                .replace("files/item", b"remote", Some(&digest(b"original")))
                .is_err()
        );
        drop(lock);
        assert_eq!(folder.read("files/item").unwrap().unwrap(), b"original");
        folder
            .replace("files/item", b"remote", Some(&digest(b"original")))
            .unwrap();

        let results = folder.parent("results", false).unwrap();
        let junction = parent.path.join("junction");
        // cmd's built-in mklink takes ordinary DOS paths.
        let dos = |path: &Path| {
            path.display()
                .to_string()
                .trim_start_matches(r"\\?\")
                .to_owned()
        };
        let command = format!(r#"mklink /J "{}" "{}""#, dos(&junction), dos(&results.path));
        assert!(
            std::process::Command::new("cmd.exe")
                .args(["/d", "/c"])
                .raw_arg(&command)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            folder
                .replace("files/junction/item", b"redirected", None)
                .is_err()
        );
        assert!(!results.path.join("item").exists());
    }

    #[cfg(windows)]
    #[test]
    fn pinned_windows_ancestors_cannot_be_renamed() {
        let (_tmp, folder, parent, _scratch) = fixture();
        let target = folder
            .parent("files", false)
            .unwrap()
            .child("nested".as_ref(), true)
            .unwrap();
        let outside = folder.root.join("moved");
        drop(parent);
        assert!(fs::rename(folder.root.join("Castles"), &outside).is_err());
        drop(target);
        fs::rename(folder.root.join("Castles"), &outside).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn interrupted_windows_updates_and_creates_report_all_surviving_artifacts() {
        let (_tmp, folder, parent, scratch) = fixture();
        let id = "00000000000000000000000000000003";
        let evidence = |kind: &str| format!("{kind}-{id}");
        let before = digest(b"original");
        let after = digest(b"replacement");
        let journal = serde_json::json!({"path":"files/item","before":before,"after":after,"parent":parent.id().unwrap()});
        write_new(
            &scratch,
            &evidence("replace"),
            &serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        write_new(&scratch, &evidence("exchange"), b"replacement").unwrap();
        write_new(&scratch, &evidence("backup"), b"original").unwrap();
        // Documented ERROR_UNABLE_TO_MOVE_REPLACEMENT_2 leaves destination absent.
        let report = folder.replacement_status().unwrap();
        assert_eq!(report[0]["destination"]["missing"], true);
        assert_eq!(report[0]["staging"]["sha256"], after);
        assert_eq!(report[0]["backup"]["sha256"], before);
        assert_eq!(folder.recoveries().unwrap().len(), 3);
        assert!(!parent.path.join("item").exists());

        for kind in ["replace", "exchange", "backup"] {
            scratch.remove(&evidence(kind)).unwrap();
        }
        let journal = serde_json::json!({"path":"files/item","before":null,"after":after,"parent":parent.id().unwrap()});
        write_new(
            &scratch,
            &evidence("replace"),
            &serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        write_new(&scratch, &evidence("stage"), b"replacement").unwrap();
        assert_eq!(
            folder.replacement_status().unwrap()[0]["staging"]["sha256"],
            after
        );
        assert_eq!(folder.recoveries().unwrap().len(), 2);
    }
}
