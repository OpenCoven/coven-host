//! Compatibility of an optional Project reference in `castle.json` (proposed, blocked on D01).
//!
//! Coven Compass #20 proposes `"project": {"source": ..., "ref": ...}` as optional, opaque
//! association metadata in the Castle manifest. These tests pin what the current v1 reader does
//! with it. They choose no Project issuer or ID: the reference below is synthetic.
use coven_host::folder::Folder;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

fn synthetic_reference() -> Value {
    json!({"source": "fixture.invalid/projects", "ref": "synthetic:project/one"})
}

fn fixture() -> (tempfile::TempDir, PathBuf, Folder) {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let root = base.join("Coven");
    let folder = Folder::new(&root, &base.join("state"));
    folder.initialize().unwrap();
    (tmp, root, folder)
}

fn write_manifest(root: &std::path::Path, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let path = root.join("Castles/demo/castle.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    let bytes = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(&path, &bytes).unwrap();
    bytes
}

#[test]
fn v1_reader_accepts_an_optional_project_reference_and_keeps_it() {
    let (_tmp, root, folder) = fixture();
    let manifest = root.join("Castles/demo/castle.json");
    for reference in [None, Some(synthetic_reference())] {
        let bytes = write_manifest(&root, |value| match reference {
            Some(reference) => value["project"] = reference,
            None => {
                value.as_object_mut().unwrap().remove("project");
            }
        });
        folder.initialize().unwrap();
        folder.replace("files/probe", b"synthetic", None).unwrap();
        assert_eq!(folder.read("files/probe").unwrap().unwrap(), b"synthetic");
        assert!(folder.scan().unwrap().files.contains_key("files/probe"));
        fs::remove_file(root.join("Castles/demo/files/probe")).unwrap();
        assert_eq!(
            fs::read(&manifest).unwrap(),
            bytes,
            "initialize must not rewrite owner content"
        );
    }
}

#[test]
fn project_reference_stays_local_and_never_syncs() {
    let (_tmp, root, folder) = fixture();
    write_manifest(&root, |value| value["project"] = synthetic_reference());
    let scan = folder.scan().unwrap();
    let synced: Vec<&String> = scan.files.keys().chain(scan.directories.iter()).collect();
    assert!(
        synced
            .iter()
            .all(|path| path.starts_with("files") || path.starts_with("results")),
        "only files/ and results/ synchronize: {synced:?}"
    );
    assert!(
        folder.read("castle.json").is_err(),
        "the manifest is not a Castle path"
    );
}

#[test]
fn version_2_is_refused_so_an_optional_field_keeps_version_1() {
    let (_tmp, root, folder) = fixture();
    write_manifest(&root, |value| {
        value["version"] = 2.into();
        value["project"] = synthetic_reference();
    });
    assert!(
        folder.scan().is_err(),
        "a version bump would break every deployed v1 reader"
    );
}

#[test]
fn project_reference_does_not_bypass_castle_identity() {
    let (_tmp, root, folder) = fixture();
    write_manifest(&root, |value| {
        value["id"] = "another-castle".into();
        value["project"] = synthetic_reference();
    });
    assert!(folder.scan().is_err());
    assert!(folder.replace("results/work", b"no", None).is_err());
}

#[test]
fn bounded_reference_fits_the_4096_byte_manifest_limit() {
    let (_tmp, root, folder) = fixture();
    // Proposed bound: source and ref at most 512 bytes each.
    write_manifest(&root, |value| {
        value["project"] = json!({"source": "s".repeat(512), "ref": "r".repeat(512)});
    });
    assert!(folder.scan().is_ok());
    // The reader refuses a manifest over its limit, so an unbounded reference would brick the Castle.
    write_manifest(&root, |value| {
        value["project"] = json!({"source": "s", "ref": "r".repeat(4096)})
    });
    assert!(folder.scan().is_err());
}
