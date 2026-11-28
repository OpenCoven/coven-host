use coven_host::{excluded, validate_path};

#[test]
fn only_canonical_public_castle_paths_are_accepted() {
    for name in ["files/a_b-9.txt", "results/deep/_x", "files/com10"] {
        assert!(validate_path(name).is_ok(), "{name}");
    }
    for name in [
        "files",
        "/files/a",
        "files/../outside",
        "files/A",
        "files/aux.txt",
        "files/a.",
        "results/vendor/a",
        "files/key.PEM",
        "files/a:b",
        "files/a%2fb",
        "files/secrets-config",
    ] {
        assert!(validate_path(name).is_err(), "{name}");
    }
    assert!(excluded("Credentials.json"));
}

#[test]
fn castle_ids_are_canonical_portable_directory_and_route_components() {
    use coven_host::CastleId;
    for id in ["a", "demo", "research-2", &"a".repeat(63)] {
        assert_eq!(id.parse::<CastleId>().unwrap().as_str(), id);
    }
    for id in [
        "",
        ".",
        "..",
        "Research",
        "a/b",
        "a%2fb",
        "a?b",
        "foo.",
        "1a",
        "-a",
        "a-",
        "nul",
        "com9",
        "lpt1",
        &"a".repeat(64),
    ] {
        assert!(id.parse::<CastleId>().is_err(), "{id}");
    }
}

#[test]
fn local_file_quota_is_counted_per_castle_across_files_and_results() -> anyhow::Result<()> {
    use coven_host::{Client, MAX_FILES};
    use std::fs;

    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    let root = base.join("Coven");
    let state = base.join("state");
    let demo = Client::new(&state);
    let research = Client::new(&state).for_castle(&"research".parse()?);
    demo.initialize(&root)?;
    research.initialize(&root)?;

    let research_files = root.join("Castles/research/files");
    for index in 0..MAX_FILES {
        fs::write(research_files.join(format!("note-{index}")), b"research")?;
    }
    fs::write(root.join("Castles/demo/files/note-0"), b"demo")?;
    assert_eq!(research.status()?["pendingPaths"], MAX_FILES);
    assert_eq!(demo.status()?["pendingPaths"], 1);

    let extra = root.join("Castles/research/results/extra");
    fs::write(&extra, b"result")?;
    let over_quota = research.status()?;
    assert!(over_quota["pendingPaths"].is_null());
    assert!(
        over_quota["error"]
            .as_str()
            .unwrap()
            .contains("local Castle exceeds")
    );
    assert_eq!(demo.status()?["pendingPaths"], 1);

    fs::remove_file(extra)?;
    assert_eq!(research.status()?["pendingPaths"], MAX_FILES);
    Ok(())
}
