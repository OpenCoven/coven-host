use coven_host::peer::{Peer, endpoint, is_precondition, is_write_rejected, serve};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
};

const TOKEN: &str = "test-token-for-peer-protocol-compatibility-123456789";

#[test]
fn server_process() {
    if let Some(storage) = std::env::var_os("COVEN_PEER_TEST_DIR") {
        serve(Path::new(&storage), 0, TOKEN.into()).unwrap();
    }
}

struct Server(Child, String);
impl Server {
    fn start(storage: &Path) -> Self {
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "server_process", "--nocapture"])
            .env("COVEN_PEER_TEST_DIR", storage)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut server = Self(child, String::new());
        let mut output = BufReader::new(server.0.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            assert!(
                output.read_line(&mut line).unwrap() > 0,
                "server exited before listening"
            );
            if let Some(addr) = line.trim().strip_prefix("Serving Castle demo on ") {
                server.1 = format!("http://{addr}");
                return server;
            }
        }
    }
    fn peer(&self) -> Peer {
        Peer::new(&self.1, TOKEN.into(), None).unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        self.0.wait().unwrap();
    }
}

#[test]
fn durable_receipt_survives_restart_and_subsequent_untracked_write() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().canonicalize().unwrap().join("storage");
    let server = Server::start(&storage);
    let mut peer = server.peer();
    assert!(peer.inventory().unwrap().is_empty());
    let pinned = peer.identity.clone();
    let op = "00112233445566778899aabbccddeeff";
    let path = "files/record.txt";
    peer.upload(path, b"first", 0, Some(op)).unwrap();
    drop(server);
    let server = Server::start(&storage);
    let mut peer = Peer::new(&server.1, TOKEN.into(), pinned.clone()).unwrap();
    peer.upload(path, b"second", 1, None).unwrap();
    peer.upload(path, b"first", 0, Some(op)).unwrap();
    let item = &peer.inventory().unwrap()[path];
    assert_eq!(peer.identity, pinned);
    assert_eq!(item.revision, 2);
    assert_eq!(peer.download(path, item).unwrap(), b"second");
    let reused = peer.upload(path, b"different", 0, Some(op)).unwrap_err();
    assert!(!is_write_rejected(&reused));
    assert!(is_precondition(
        &peer.upload(path, b"stale", 0, None).unwrap_err()
    ));
    assert!(is_write_rejected(
        &peer
            .upload("files/record.txt/more", b"", 0, None)
            .unwrap_err()
    ));
}

#[test]
fn authority_requires_verified_tls_or_literal_ipv4_tunnel() {
    for url in [
        "http://127.0.0.1:1234/",
        "https://peer.example",
        "https://[::1]:8443",
    ] {
        endpoint(url).unwrap();
    }
    for url in [
        "http://example.com",
        "http://localhost",
        "http://127.1",
        "http://127.0.0.1:0",
        "https://user@host",
        "https://host/?",
        "https://host/path/..",
        "https://host/#",
    ] {
        assert!(endpoint(url).is_err(), "{url}");
    }
}

#[test]
fn failed_receipt_insert_rolls_back_bytes_and_revision() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().canonicalize().unwrap().join("storage");
    let server = Server::start(&storage);
    let mut peer = server.peer();
    peer.inventory().unwrap();
    peer.upload("results/file", b"original", 0, None).unwrap();
    let db = rusqlite::Connection::open(storage.join("host.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_receipt BEFORE INSERT ON receipt
        BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;",
    )
    .unwrap();
    let op = "00000000000000000000000000000001";
    let error = peer
        .upload("results/file", b"changed", 1, Some(op))
        .unwrap_err();
    assert!(!is_write_rejected(&error));
    let item = &peer.inventory().unwrap()["results/file"];
    assert_eq!(item.revision, 1);
    assert_eq!(peer.download("results/file", item).unwrap(), b"original");
    db.execute_batch("DROP TRIGGER fail_receipt").unwrap();
    peer.upload("results/file", b"changed", 1, Some(op))
        .unwrap();
    assert_eq!(peer.inventory().unwrap()["results/file"].revision, 2);
}

#[test]
fn deletion_receipt_survives_restart_and_recreation() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().canonicalize().unwrap().join("storage");
    let server = Server::start(&storage);
    let mut peer = server.peer();
    peer.inventory().unwrap();
    let path = "files/note";
    let op = "00000000000000000000000000000002";
    peer.upload(path, b"archive me", 0, None).unwrap();
    peer.delete(path, 1, op).unwrap();
    drop(server);
    let server = Server::start(&storage);
    let mut peer = Peer::new(&server.1, TOKEN.into(), peer.identity).unwrap();
    assert!(peer.inventory().unwrap().is_empty());
    assert!(is_precondition(
        &peer.upload(path, b"stale creation", 0, None).unwrap_err()
    ));
    peer.upload(path, b"new version", 2, None).unwrap();
    peer.delete(path, 1, op).unwrap();
    let item = &peer.inventory().unwrap()[path];
    assert_eq!(item.revision, 3);
    assert_eq!(peer.download(path, item).unwrap(), b"new version");
    let db = rusqlite::Connection::open(storage.join("host.sqlite")).unwrap();
    let retained: Vec<u8> = db
        .query_row(
            "SELECT bytes FROM deleted_versions WHERE path=? AND revision=2",
            [path],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(retained, b"archive me");
}
