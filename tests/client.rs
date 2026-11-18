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
