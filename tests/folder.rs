use coven_host::{
    digest,
    folder::{Folder, private_dir, read_external, token_from},
};
use std::{fs, path::PathBuf};

fn fixture() -> (tempfile::TempDir, PathBuf, Folder) {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let root = base.join("Coven");
    let folder = Folder::new(&root, &base.join("state"));
    folder.initialize().unwrap();
    (tmp, root.join("Castles/demo"), folder)
}

#[test]
fn creates_once_updates_only_expected_bytes_and_scans_nested_files() {
    let (_tmp, castle, folder) = fixture();
    folder.replace("files/sub/item", b"one", None).unwrap();
    assert!(
        folder
            .replace("files/sub/item", b"unexpected", None)
            .is_err()
    );
    assert!(
        folder
            .replace("files/sub/item", b"unexpected", Some(&digest(b"wrong")))
            .is_err()
    );
    assert_eq!(fs::read(castle.join("files/sub/item")).unwrap(), b"one");
    folder
        .replace("files/sub/item", b"two", Some(&digest(b"one")))
        .unwrap();
    assert_eq!(folder.read("files/sub/item").unwrap().unwrap(), b"two");
    fs::write(castle.join("files/sub/token-me"), b"private").unwrap();
    let scan = folder.scan().unwrap();
    assert_eq!(scan.files.len(), 1);
    assert_eq!(scan.files["files/sub/item"], (digest(b"two"), 3));
    assert!(scan.directories.contains("files/sub"));
    assert!(folder.recoveries().unwrap().is_empty());
}

#[test]
fn refuses_wrong_manifest_and_case_aliases() {
    let (_tmp, castle, folder) = fixture();
    fs::write(castle.join("files/NOTE"), b"owner content").unwrap();
    if castle.join("files/note").exists() {
        assert!(folder.read("files/note").is_err());
        assert!(
            folder
                .replace("files/note", b"remote", Some(&digest(b"owner content")))
                .is_err()
        );
    }
    assert!(folder.scan().is_err());
    fs::write(
        castle.join("castle.json"),
        br#"{"version":true,"id":"demo"}"#,
    )
    .unwrap();
    assert!(folder.replace("results/work", b"no", None).is_err());
}

#[cfg(unix)]
#[test]
fn rejects_links_and_checks_permission_on_the_open_token() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_tmp, castle, folder) = fixture();
    let token = castle.join("private-token");
    fs::write(&token, "x".repeat(43)).unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(token_from(&token).is_err());
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(token_from(&token).unwrap().len(), 43);
    assert!(read_external(&token, 2).is_err());
    symlink(&token, castle.join("files/link")).unwrap();
    assert!(folder.read("files/link").is_err());
    fs::hard_link(&token, castle.join("files/hard")).unwrap();
    assert!(folder.read("files/hard").is_err());
    symlink(castle.join("results"), castle.join("files/dir")).unwrap();
    assert!(folder.replace("files/dir/out", b"x", None).is_err());
    assert!(!castle.join("results/out").exists());
    assert!(private_dir(&castle.join("files/dir")).is_err());
}

#[cfg(windows)]
#[test]
fn rejects_windows_token_after_another_principal_gains_read_access() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap().join("credentials");
    private_dir(&root).unwrap();
    let path = root.join("token");
    fs::write(&path, "A".repeat(43)).unwrap();
    assert_eq!(token_from(&path).unwrap(), "A".repeat(43));
    assert!(read_external(&path, 10).is_err());
    // The well-known Everyone SID works on non-English Windows installations too.
    let dos = path.display().to_string();
    let acl = std::process::Command::new("icacls.exe")
        .args([dos.trim_start_matches(r"\\?\"), "/grant", "*S-1-1-0:(R)"])
        .output()
        .unwrap();
    assert!(
        acl.status.success(),
        "{}",
        String::from_utf8_lossy(&acl.stderr)
    );
    assert!(token_from(&path).is_err());
    let folder = Folder::new(&root.join("Coven"), &root.join("state"));
    folder.initialize().unwrap();
    fs::rename(&path, root.join("Coven/Castles/demo/files/shared")).unwrap();
    folder
        .replace("files/shared", b"new", Some(&digest(&[b'A'; 43])))
        .unwrap();
    let saved = folder.previous_versions().unwrap();
    assert_eq!(token_from(&saved[0]).unwrap(), "A".repeat(43));
}

#[test]
fn late_save_through_open_handle_survives_multiple_downloads() {
    use std::io::{Seek, Write};
    let (tmp, castle, folder) = fixture();
    let local = castle.join("files/note.md");
    fs::write(&local, b"base").unwrap();
    let mut options = fs::OpenOptions::new();
    options.write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    }
    let mut first_editor = options.open(&local).unwrap();
    folder
        .replace("files/note.md", b"next", Some(&digest(b"base")))
        .unwrap();
    folder
        .replace("files/note.md", b"base", Some(&digest(b"next")))
        .unwrap();
    let mut second_editor = options.open(&local).unwrap();
    folder
        .replace("files/note.md", b"final", Some(&digest(b"base")))
        .unwrap();
    for (handle, data) in [
        (&mut first_editor, b"late one"),
        (&mut second_editor, b"late two"),
    ] {
        handle.rewind().unwrap();
        handle.write_all(data).unwrap();
        handle.sync_all().unwrap();
    }
    assert_eq!(fs::read(&local).unwrap(), b"final");
    let saved = folder.previous_versions().unwrap();
    assert!(saved.iter().all(|p| {
        p.parent()
            .unwrap()
            .ends_with("previous-versions/files/note.md")
    }));
    assert_eq!(saved.len(), 3);
    let baselines: Vec<_> = saved
        .iter()
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .contains(&digest(b"base"))
        })
        .map(|p| fs::read(p).unwrap())
        .collect();
    assert!(baselines.contains(&b"late one".to_vec()) && baselines.contains(&b"late two".to_vec()));
    assert!(folder.recoveries().unwrap().is_empty());
    assert_eq!(
        fs::read_dir(tmp.path().join("state/recovery"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn full_previous_versions_directory_preserves_current_file() {
    let (tmp, castle, folder) = fixture();
    let archive = tmp
        .path()
        .canonicalize()
        .unwrap()
        .join("state/previous-versions");
    private_dir(&archive).unwrap();
    let huge = fs::File::create(archive.join("retained")).unwrap();
    huge.set_len(coven_host::MAX_TOTAL as u64).unwrap();
    drop(huge);
    let dest = castle.join("files/item");
    fs::write(&dest, b"old").unwrap();
    let err = folder
        .replace("files/item", b"new", Some(&digest(b"old")))
        .unwrap_err();
    assert!(err.to_string().contains("move") && err.to_string().contains("previous-versions"));
    assert_eq!(fs::read(&dest).unwrap(), b"old");
    assert!(folder.recoveries().unwrap().is_empty());
}
