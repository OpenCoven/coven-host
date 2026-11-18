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
}
