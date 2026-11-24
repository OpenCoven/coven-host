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
fn profile_claims_serialize_and_survive_restart() {
    use std::sync::Barrier;
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("castle");
    let barrier = Barrier::new(2);
    let candidates = [base.join("one"), base.join("two")];
    let results = std::thread::scope(|scope| {
        candidates
            .each_ref()
            .map(|home| {
                let barrier = &barrier;
                let root = &root;
                scope.spawn(move || {
                    let client = Client::for_profile(&home.join("state"), home);
                    barrier.wait();
                    client.initialize(root)
                })
            })
            .map(|handle| handle.join().unwrap())
    });
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "{results:?}"
    );
    let winner = results.iter().position(Result::is_ok).unwrap();
    let client =
        |index: usize| Client::for_profile(&candidates[index].join("state"), &candidates[index]);
    let path = root.join("Castles/demo/files/draft");
    fs::write(&path, b"offline edit").unwrap();
    assert!(
        client(1 - winner)
            .initialize(&root.join("."))
            .unwrap_err()
            .to_string()
            .contains("different daemon profile")
    );
    client(winner).initialize(&root).unwrap();
    assert_eq!(client(winner).status().unwrap()["pendingPaths"], 1);
    assert_eq!(fs::read(path).unwrap(), b"offline edit");
}

#[test]
fn legacy_profile_requires_explicit_selection_before_adoption() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("castle");
    let state = base.join("original/state");
    Client::new(&state).initialize(&root).unwrap();
    let profile = Client::for_profile(&state, &base.join("original"));
    assert!(
        profile
            .status()
            .unwrap_err()
            .to_string()
            .contains("unverified")
    );
    let other = Client::for_profile(&base.join("other/state"), &base.join("other"));
    assert!(other.initialize(&root).is_err());
    profile.initialize(&root).unwrap();
    let shared_state = Client::for_profile(&state, &base.join("other"));
    assert!(shared_state.initialize(&root).is_err());
    let db = rusqlite::Connection::open(state.join("client.sqlite")).unwrap();
    db.execute_batch("DROP TABLE deleted_baseline; DROP TABLE pending_delete;")
        .unwrap();
    assert!(
        shared_state
            .status()
            .unwrap_err()
            .to_string()
            .contains("different daemon profile")
    );
    let deletion_tables: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE name IN ('deleted_baseline','pending_delete')",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(
        deletion_tables, 0,
        "refused profile migrated the owner's state"
    );
    profile.status().unwrap();
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

#[test]
fn two_castles_share_root_ownership_but_preserve_independent_local_state() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("Coven");
    let state = base.join("profile/host");
    let profile = || Client::for_profile(&state, &base.join("profile"));
    let research = || profile().for_castle(&"research".parse().unwrap());
    Client::new(&state).initialize(&root).unwrap();
    assert!(
        research().initialize(&root).is_err(),
        "adopt legacy demo before adding another Castle"
    );
    profile().initialize(&root).unwrap();
    research().initialize(&root).unwrap();
    let manifest = root.join("Castles/research/castle.json");
    let metadata =
        br#"{"version":1,"id":"research","project":{"id":"local-reference"},"extra":[1,2]}"#;
    fs::write(&manifest, metadata).unwrap();
    research().initialize(&root).unwrap();
    assert_eq!(fs::read(&manifest).unwrap(), metadata);
    fs::write(root.join("Castles/demo/files/note.md"), b"demo").unwrap();
    assert_eq!(profile().status().unwrap()["pendingPaths"], 1);
    let status = research().status().unwrap();
    assert_eq!(status["castle"], "research");
    assert_eq!(status["pendingPaths"], 0);
    assert_ne!(profile().status().unwrap()["recovery"], status["recovery"]);
    let other = Client::for_profile(&base.join("other/host"), &base.join("other"))
        .for_castle(&"third".parse().unwrap());
    assert!(other.initialize(&root).is_err());
    assert!(!root.join("Castles/third").exists());
    fs::write(&manifest, br#"{"version":1,"id":"demo"}"#).unwrap();
    assert_eq!(research().status().unwrap()["phase"], "needs attention");
    assert_eq!(profile().status().unwrap()["pendingPaths"], 1);
}
