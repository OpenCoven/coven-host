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
