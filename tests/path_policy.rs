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
