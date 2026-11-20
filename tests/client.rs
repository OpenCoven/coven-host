use coven_host::Client;
use std::fs;

#[test]
fn offline_status_detects_local_work_and_init_preserves_owner_content() {
    let temp = tempfile::tempdir().unwrap();
    let temp = temp.path().canonicalize().unwrap();
    let root = temp.join("Coven");
    let client = Client::new(&temp.join("state"));
    client.initialize(&root).unwrap();
    fs::write(root.join("README.md"), "my notes").unwrap();
    client.initialize(&root).unwrap();
    assert_eq!(fs::read(root.join("README.md")).unwrap(), b"my notes");
    fs::write(root.join("Castles/demo/files/local.md"), "work").unwrap();
    assert_eq!(client.status().unwrap()["pendingPaths"], 1);
    assert!(client.initialize(&temp.join("other")).is_err());
}

#[test]
fn overlapping_state_and_root_are_rejected_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("Coven");
    assert!(Client::new(&root.join("state")).initialize(&root).is_err());
    assert!(!root.exists());
}

#[cfg(unix)]
#[test]
fn a_link_cannot_redirect_the_client_database() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let state = base.join("state");
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let other = base.join("unrelated.sqlite");
    fs::write(&other, b"").unwrap();
    symlink(&other, state.join("client.sqlite")).unwrap();
    assert!(Client::new(&state).initialize(&base.join("Coven")).is_err());
    assert!(fs::read(other).unwrap().is_empty());
}

#[test]
fn offline_status_identifies_bytes_saved_through_a_displaced_open_file() {
    use coven_host::{digest, folder::Folder};
    use std::io::{Seek, Write};
    use std::path::Path;
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let state = base.join("state");
    let root = base.join("Coven");
    let client = Client::new(&state);
    client.initialize(&root).unwrap();
    let folder = Folder::new(&root, &state);
    let key = "files/sub/document";
    folder.replace(key, b"original", None).unwrap();
    let path = root.join("Castles/demo").join(key);
    let mut editor = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .unwrap();
    folder
        .replace(key, b"downloaded", Some(&digest(b"original")))
        .unwrap();
    editor.rewind().unwrap();
    editor.write_all(b"edited after replacement").unwrap();
    editor.sync_all().unwrap();
    drop(editor);
    let status = Client::new(&state).status().unwrap();
    let versions = status["previousVersions"]
        .as_array()
        .expect("status exposes saved versions");
    assert_eq!(versions.len(), 1);
    let copy = Path::new(versions[0].as_str().unwrap());
    assert!(copy.parent().unwrap().ends_with(key));
    assert_eq!(fs::read(copy).unwrap(), b"edited after replacement");
    assert_eq!(fs::read(&path).unwrap(), b"downloaded");
}

#[test]
fn offline_reopen_adds_deletion_tables_and_reports_outstanding_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("Castle");
    let state = base.join("state");
    let client = Client::new(&state);
    client.initialize(&root).unwrap();
    let db = rusqlite::Connection::open(state.join("client.sqlite")).unwrap();
    db.execute_batch("DROP TABLE deleted_baseline; DROP TABLE pending_delete;")
        .unwrap();
    db.execute(
        "INSERT INTO baseline VALUES (?1,1,?2)",
        ("files/old", coven_host::digest(b"old")),
    )
    .unwrap();
    let reopened = Client::new(&state);
    assert_eq!(reopened.status().unwrap()["pendingPaths"], 1);
    db.execute(
        "INSERT INTO pending_delete VALUES (1,'files/old',1,?1)",
        ["1234567890abcdef1234567890abcdef"],
    )
    .unwrap();
    let status = reopened.status().unwrap();
    assert_eq!(status["awaitingAcknowledgement"], "files/old");
    assert_eq!(status["pendingPaths"], 1);
    assert!(!root.join("Castles/demo/files/old").exists());
}
