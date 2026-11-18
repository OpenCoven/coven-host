use super::{missing_or, read_at};
use crate::{MAX_FILE, digest};
use anyhow::{Context, Result, ensure};
use rustix::fs::{self as at, AtFlags, FileType, Mode, OFlags, RenameFlags};
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
use std::path::{Component, Path, PathBuf};

pub(super) struct Dir {
    pub file: File,
    pub path: PathBuf,
}

fn std_error(e: rustix::io::Errno) -> io::Error {
    io::Error::from_raw_os_error(e.raw_os_error())
}

const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NOFOLLOW);

impl Dir {
    pub fn open(path: &Path, create: bool) -> Result<Self> {
        let path = std::path::absolute(path)?;
        let mut dir = Self {
            file: File::open("/")?,
            path: "/".into(),
        };
        for component in path.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(name) => dir = dir.child(name, create)?,
                _ => anyhow::bail!("unsafe directory component in {}", path.display()),
            }
        }
        Ok(dir)
    }

    pub fn child(&self, name: &OsStr, create: bool) -> Result<Self> {
        if create {
            match at::mkdirat(&self.file, name, Mode::RWXU) {
                Ok(()) => self.sync()?,
                Err(rustix::io::Errno::EXIST) => (),
                Err(e) => return Err(std_error(e).into()),
            }
        }
        let file =
            File::from(at::openat(&self.file, name, DIRECTORY, Mode::empty()).map_err(std_error)?);
        self.check_name(&file, name)?;
        Ok(Self {
            file,
            path: self.path.join(name),
        })
    }

    pub fn id(&self) -> Result<(u64, u64)> {
        let meta = self.file.metadata()?;
        Ok((meta.dev(), meta.ino()))
    }

    pub fn each_name(&self, mut visit: impl FnMut(&OsStr) -> Result<()>) -> Result<()> {
        for entry in at::Dir::read_from(&self.file).map_err(std_error)? {
            let entry = entry.map_err(std_error)?;
            let bytes = entry.file_name().to_bytes();
            if bytes != b"." && bytes != b".." {
                visit(OsStr::from_bytes(bytes))?;
            }
        }
        Ok(())
    }

    fn check_name(&self, file: &File, name: &OsStr) -> Result<()> {
        // Compare the name resolved by the kernel, including on case-insensitive volumes.
        #[cfg(target_os = "macos")]
        {
            use std::os::fd::AsRawFd;
            let mut bytes = [0u8; libc::MAXPATHLEN as usize];
            let result =
                unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, bytes.as_mut_ptr()) };
            if result == -1 {
                return Err(io::Error::last_os_error().into());
            }
            let length = bytes
                .iter()
                .position(|b| *b == 0)
                .context("invalid resolved path")?;
            ensure!(
                Path::new(OsStr::from_bytes(&bytes[..length])).file_name() == Some(name),
                "{name:?}: path alias or moved entry"
            );
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;
            // procfs reports the dentry spelling. Fall back to directory enumeration on
            // systems without procfs, preserving the same case-sensitive comparison.
            match std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())) {
                Ok(resolved) => ensure!(
                    resolved.file_name() == Some(name),
                    "{name:?}: path alias or moved entry"
                ),
                Err(_) => {
                    let mut found = false;
                    self.each_name(|entry| {
                        found |= entry == name;
                        Ok(())
                    })?;
                    ensure!(found, "{name:?}: path alias or moved entry");
                }
            }
        }
        Ok(())
    }

    pub fn is_dir(&self, name: &OsStr) -> Result<bool> {
        let meta = at::statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW).map_err(std_error)?;
        Ok(FileType::from_raw_mode(meta.st_mode) == FileType::Directory)
    }

    pub fn open_file(&self, name: &OsStr) -> Result<File> {
        let info = at::statat(&self.file, name, AtFlags::SYMLINK_NOFOLLOW).map_err(std_error)?;
        ensure!(
            FileType::from_raw_mode(info.st_mode) == FileType::RegularFile && info.st_nlink == 1,
            "{name:?}: expected a regular file (no symlinks or hard links)"
        );
        let file = File::from(
            at::openat(
                &self.file,
                name,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
                Mode::empty(),
            )
            .map_err(std_error)?,
        );
        let meta = file.metadata()?;
        ensure!(
            meta.dev() == info.st_dev as u64 && meta.ino() == info.st_ino as u64,
            "{name:?}: file changed while opening"
        );
        self.check_name(&file, name)?;
        Ok(file)
    }

    pub fn create(&self, name: &OsStr) -> Result<File> {
        Ok(File::from(
            at::openat(
                &self.file,
                name,
                OFlags::WRONLY | OFlags::CLOEXEC | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(std_error)?,
        ))
    }

    pub fn sync(&self) -> Result<()> {
        Ok(self.file.sync_all()?)
    }

    pub fn remove(&self, name: &str) -> Result<()> {
        at::unlinkat(&self.file, name, AtFlags::empty()).map_err(std_error)?;
        self.sync()
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct Fingerprint {
    pub size: u64,
    dev: u64,
    ino: u64,
    mode: u32,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

pub(super) fn fingerprint(file: &File) -> Result<Fingerprint> {
    let m = file.metadata()?;
    ensure!(
        m.is_file() && m.nlink() == 1,
        "expected a singly linked regular file"
    );
    Ok(Fingerprint {
        size: m.size(),
        dev: m.dev(),
        ino: m.ino(),
        mode: m.mode(),
        mtime: (m.mtime(), m.mtime_nsec()),
        ctime: (m.ctime(), m.ctime_nsec()),
    })
}

pub(super) fn check_private(file: &File) -> Result<()> {
    let m = file.metadata()?;
    ensure!(
        m.uid() == unsafe { libc::geteuid() } && m.mode() & 0o077 == 0,
        "expected a private file or directory owned by you (chmod 600 or 700)"
    );
    Ok(())
}

pub(super) fn install(
    parent: &Dir,
    name: &str,
    scratch: &Dir,
    stage: &str,
    expected: Option<&str>,
    path: &str,
    _data: &[u8],
) -> Result<()> {
    let verify_parent = || -> Result<()> {
        let live = Dir::open(&parent.path, false)?;
        ensure!(
            live.id()? == parent.id()?,
            "{path}: parent moved during transfer; retry"
        );
        Ok(())
    };
    if let Err(e) = verify_parent() {
        scratch.remove(stage)?;
        return Err(e);
    }
    let flags = if expected.is_some() {
        RenameFlags::EXCHANGE
    } else {
        RenameFlags::NOREPLACE
    };
    if let Err(error) = at::renameat_with(&scratch.file, stage, &parent.file, name, flags) {
        scratch.remove(stage)?;
        return Err(std_error(error)).with_context(|| format!("atomic replacement of {path}"));
    }
    #[cfg(test)]
    super::tests::exit_at_publication();
    parent.sync()?;
    scratch.sync()?;
    // A Unix directory can still move while the kernel performs the rename.
    // Retain the displaced inode until the configured parent has been re-opened.
    verify_parent().with_context(|| {
        format!(
            "{path}: inspect moved directory and recovery at {}",
            scratch.path.display()
        )
    })?;
    if let Some(expected) = expected {
        let recovery = scratch.path.join(stage);
        let displaced = missing_or(read_at(scratch, stage.as_ref(), MAX_FILE, false))
            .with_context(|| format!("{path}: inspect {}", recovery.display()))?;
        ensure!(
            displaced.is_some_and(|bytes| digest(&bytes) == expected),
            "{path}: an edit raced replacement; recover it from {}",
            recovery.display()
        );
        scratch.remove(stage)?;
    }
    Ok(())
}
