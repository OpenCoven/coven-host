use super::{Folder, missing_or, read_at, write_new};
use crate::{MAX_FILE, digest, validate_path};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::{
    ffi::{OsStrExt, OsStringExt},
    fs::OpenOptionsExt,
    io::AsRawHandle,
};
use std::path::{Component, Path, PathBuf, Prefix};
use std::ptr::{null, null_mut};
use std::sync::Arc;
use windows_sys::Win32::Storage::FileSystem::*;

#[path = "windows_security.rs"]
mod security;
pub(super) use security::check_private;

pub(super) fn wide(path: &OsStr) -> Result<Vec<u16>> {
    let mut w: Vec<u16> = path.encode_wide().collect();
    ensure!(!w.contains(&0), "embedded NUL in path");
    w.push(0);
    Ok(w)
}

fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    ensure!(
        info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "reparse points and junctions are unsafe"
    );
    Ok(info)
}

fn check_spelling(file: &File, name: &OsStr) -> Result<()> {
    let handle = file.as_raw_handle();
    let mut size =
        unsafe { GetFinalPathNameByHandleW(handle, null_mut(), 0, FILE_NAME_NORMALIZED) };
    loop {
        if size == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let mut buf = vec![0; size as usize];
        let n = unsafe {
            GetFinalPathNameByHandleW(handle, buf.as_mut_ptr(), size, FILE_NAME_NORMALIZED)
        };
        if n == 0 {
            return Err(io::Error::last_os_error().into());
        }
        if n < size {
            let canonical = PathBuf::from(OsString::from_wide(&buf[..n as usize]));
            ensure!(
                canonical.file_name().is_some_and(|actual| actual == name),
                "{name:?}: path alias or moved entry"
            );
            return Ok(());
        }
        size = n;
    }
}

fn valid_component(name: &OsStr) -> Result<()> {
    let s = name.to_string_lossy();
    ensure!(
        !s.is_empty()
            && !s.ends_with([' ', '.'])
            && !s.contains([':', '/', '\\', '?', '*', '"', '<', '>', '|'])
            && !s.chars().any(char::is_control),
        "unsafe Windows path component {name:?}"
    );
    Ok(())
}

// Keep every ancestor open without sharing rename/delete or writes to the
// directory itself. ReplaceFileW takes names, so pinning just the last parent
// would allow a junction to be substituted higher up the tree.
struct Ancestor {
    _file: File,
    _parent: Option<Arc<Ancestor>>,
}

pub(super) struct Dir {
    pub file: File,
    pub path: PathBuf,
    ancestors: Option<Arc<Ancestor>>,
}

fn open_directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES | READ_CONTROL)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    ensure!(
        info(&file)?.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
        "expected a directory: {}",
        path.display()
    );
    Ok(file)
}

impl Dir {
    pub fn open(path: &Path, create: bool) -> Result<Self> {
        ensure!(
            !path.components().any(|p| p == Component::ParentDir),
            "unsafe parent component"
        );
        let absolute = std::path::absolute(path)?;
        let mut parts = absolute.components();
        let Some(Component::Prefix(prefix)) = parts.next() else {
            anyhow::bail!("expected a local volume path")
        };
        let drive = match prefix.kind() {
            Prefix::VerbatimDisk(letter) | Prefix::Disk(letter) => letter as char,
            _ => anyhow::bail!("expected a local drive path"),
        };
        ensure!(
            parts.next() == Some(Component::RootDir),
            "expected absolute drive path"
        );
        let path: PathBuf = format!("\\\\?\\{drive}:\\").into();
        let mut dir = Dir {
            file: open_directory(&path)?,
            path,
            ancestors: None,
        };
        for part in parts {
            let Component::Normal(name) = part else {
                anyhow::bail!("unsafe directory component")
            };
            dir = dir.child(name, create)?;
        }
        Ok(dir)
    }

    pub fn child(&self, name: &OsStr, create: bool) -> Result<Self> {
        valid_component(name)?;
        let path = self.path.join(name);
        let file = match open_directory(&path) {
            Err(e)
                if create
                    && e.downcast_ref::<io::Error>()
                        .is_some_and(|e| e.kind() == io::ErrorKind::NotFound) =>
            {
                security::mkdir(&path)?;
                open_directory(&path)?
            }
            other => other?,
        };
        check_spelling(&file, name)?;
        let ancestors = Some(Arc::new(Ancestor {
            _file: self.file.try_clone()?,
            _parent: self.ancestors.clone(),
        }));
        Ok(Self {
            file,
            path,
            ancestors,
        })
    }

    pub fn id(&self) -> Result<(u64, u64)> {
        let m = info(&self.file)?;
        Ok((
            m.dwVolumeSerialNumber as u64,
            pair(m.nFileIndexHigh, m.nFileIndexLow),
        ))
    }

    pub fn each_name(&self, mut visit: impl FnMut(&OsStr) -> Result<()>) -> Result<()> {
        for e in std::fs::read_dir(&self.path)? {
            visit(&e?.file_name())?;
        }
        Ok(())
    }

    pub fn is_dir(&self, name: &OsStr) -> Result<bool> {
        valid_component(name)?;
        Ok(std::fs::symlink_metadata(self.path.join(name))?.is_dir())
    }

    pub fn open_file(&self, name: &OsStr) -> Result<File> {
        valid_component(name)?;
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(self.path.join(name))?;
        fingerprint(&file)?;
        check_spelling(&file, name)?;
        Ok(file)
    }

    pub fn create(&self, name: &OsStr) -> Result<File> {
        valid_component(name)?;
        Ok(OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_WRITE_THROUGH)
            .open(self.path.join(name))?)
    }

    // NTFS directory handles do not provide a portable directory flush. New
    // files are flushed individually; journal publication uses a write-through move.
    pub fn sync(&self) -> Result<()> {
        Ok(())
    }

    pub fn open_lock(&self, name: &OsStr) -> Result<File> {
        valid_component(name)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(self.path.join(name))?;
        fingerprint(&file)?;
        check_spelling(&file, name)?;
        Ok(file)
    }

    pub fn move_new(&self, source: &str, dest: &Dir, name: &str) -> Result<()> {
        // A same-volume move preserves the original file DACL. A private parent
        // alone does not exclude Windows principals with traverse-check bypass.
        let file = self.open_file(source.as_ref())?;
        if security::check_private(&file).is_err() {
            let writable = OpenOptions::new()
                .access_mode(WRITE_DAC | READ_CONTROL | FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(self.path.join(source))?;
            ensure!(
                fingerprint(&file)? == fingerprint(&writable)?,
                "backup changed while securing it"
            );
            security::protect(&writable)?;
        }
        drop(file);
        move_new(&self.path.join(source), &dest.path.join(name))
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        Ok(std::fs::remove_file(self.path.join(name))?)
    }
}

fn pair(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

#[derive(PartialEq, Eq)]
pub(super) struct Fingerprint {
    pub size: u64,
    volume: u32,
    id: u64,
    changed: i64,
    modified: i64,
}

pub(super) fn fingerprint(file: &File) -> Result<Fingerprint> {
    let m = info(file)?;
    ensure!(
        m.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            && m.nNumberOfLinks == 1
            && unsafe { GetFileType(file.as_raw_handle()) } == FILE_TYPE_DISK,
        "expected a singly linked regular file"
    );
    let mut times: FILE_BASIC_INFO = unsafe { zeroed() };
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&mut times as *mut FILE_BASIC_INFO).cast(),
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    Ok(Fingerprint {
        size: pair(m.nFileSizeHigh, m.nFileSizeLow),
        volume: m.dwVolumeSerialNumber,
        id: pair(m.nFileIndexHigh, m.nFileIndexLow),
        changed: times.ChangeTime,
        modified: times.LastWriteTime,
    })
}

fn move_new(from: &Path, to: &Path) -> Result<()> {
    let (from, to) = (wide(from.as_os_str())?, wide(to.as_os_str())?);
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Journal {
    path: String,
    before: Option<String>,
    after: String,
    parent: (u64, u64),
}

fn file_hash(dir: &Dir, name: &str) -> Result<Option<String>> {
    Ok(missing_or(read_at(dir, name.as_ref(), MAX_FILE, false))?.map(|data| digest(&data)))
}

pub(super) fn install(
    parent: &Dir,
    name: &str,
    scratch: &Dir,
    stage: &str,
    expected: Option<&str>,
    path: &str,
    data: &[u8],
) -> Result<()> {
    let target = parent.path.join(name);
    let source = scratch.path.join(stage);
    let stage_prefix = if expected.is_some() {
        "exchange-"
    } else {
        "stage-"
    };
    let id = stage
        .strip_prefix(stage_prefix)
        .context("invalid staging name")?;
    let journal = format!("replace-{id}");
    let backup = format!("backup-{id}");
    let pending = format!("prepare-{id}");
    let previous = expected
        .map(|hash| {
            super::previous::PreviousVersion::reserve(parent, name, scratch, stage, path, hash)
        })
        .transpose()?;
    let record = Journal {
        path: path.into(),
        before: expected.map(str::to_owned),
        after: digest(data),
        parent: parent.id()?,
    };
    write_new(scratch, &pending, &serde_json::to_vec(&record)?)?;
    move_new(&scratch.path.join(&pending), &scratch.path.join(&journal))?;
    let (Some(before), Some(previous)) = (expected, previous) else {
        let result = move_new(&source, &target);
        if result.is_err() {
            // Check both names before discarding an unpublished private transfer.
            let staged = file_hash(scratch, stage);
            let destination = file_hash(parent, name);
            if matches!(staged, Ok(Some(ref hash)) if hash==&record.after) && destination.is_ok() {
                scratch.remove(stage)?;
                scratch.remove(&journal)?;
            }
            return result;
        }
        scratch.remove(&journal)?;
        return Ok(());
    };
    let dest = wide(target.as_os_str())?;
    let src = wide(source.as_os_str())?;
    let bak = wide(scratch.path.join(&backup).as_os_str())?;
    let ok = unsafe { ReplaceFileW(dest.as_ptr(), src.as_ptr(), bak.as_ptr(), 0, null(), null()) };
    let error = (ok == 0).then(io::Error::last_os_error);
    #[cfg(test)]
    if ok != 0 {
        super::tests::exit_at_publication();
    }
    let destination = file_hash(parent, name);
    let staged = file_hash(scratch, stage);
    let displaced = file_hash(scratch, &backup);
    if let (Ok(destination), Ok(staged), Ok(displaced)) = (destination, staged, displaced) {
        if destination.as_deref() == Some(&record.after)
            && staged.is_none()
            && displaced.as_deref() == Some(before)
        {
            previous.promote(scratch, &backup)?;
            scratch.remove(&journal)?;
            return Ok(());
        }
        if destination.as_deref() == Some(before)
            && staged.as_deref() == Some(&record.after)
            && displaced.is_none()
            && let Some(e) = error
        {
            scratch.remove(stage)?;
            scratch.remove(&journal)?;
            return Err(e).context("file replacement failed before publication; retry");
        }
    }
    let retained = scratch.path.join(&journal);
    match error {
        Some(e) => Err(e).with_context(|| {
            format!(
                "{path}: replacement interrupted; inspect {}",
                retained.display()
            )
        }),
        None => anyhow::bail!(
            "{path}: an edit raced replacement; inspect {}",
            retained.display()
        ),
    }
}

fn observe(h: Result<Option<String>>) -> Value {
    match h {
        Ok(Some(hash)) => json!({"sha256":hash}),
        Ok(None) => json!({"missing":true}),
        Err(error) => json!({"error":error.to_string()}),
    }
}

pub(super) fn inspect(folder: &Folder, dir: &Dir, name: &str) -> Result<Value> {
    let record: Journal = serde_json::from_slice(&read_at(dir, name.as_ref(), 4096, true)?)?;
    validate_path(&record.path)?;
    let id = name
        .strip_prefix("replace-")
        .context("invalid journal name")?;
    let (relative, leaf) = record
        .path
        .rsplit_once('/')
        .context("invalid journal path")?;
    let destination = folder.parent(relative, false).and_then(|parent| {
        ensure!(
            parent.id()? == record.parent,
            "destination parent has moved"
        );
        file_hash(&parent, leaf)
    });
    let stage = format!(
        "{}-{id}",
        if record.before.is_some() {
            "exchange"
        } else {
            "stage"
        }
    );
    let backup = format!("backup-{id}");
    let previous_path = record
        .before
        .as_deref()
        .map(|hash| -> Result<PathBuf> {
            Ok(super::previous::archive_path(dir)?
                .join(&record.path)
                .join(super::previous::name(id, hash)?))
        })
        .transpose()?;
    let previous = previous_path.as_ref().map(|p| {
        observe(
            missing_or(super::read_external(p, MAX_FILE)).map(|v| v.map(|bytes| digest(&bytes))),
        )
    });
    Ok(
        json!({"journal":dir.path.join(name),"path":record.path,"expected":record.before,"incoming":record.after,
        "destination":observe(destination),"stagingPath":dir.path.join(&stage),"staging":observe(file_hash(dir,&stage)),
        "backupPath":dir.path.join(&backup),"backup":observe(file_hash(dir,&backup)), "previousPath":previous_path, "previous":previous}),
    )
}
