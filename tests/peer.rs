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
        if std::env::var_os("COVEN_TEST_NAMED").is_some() {
            coven_host::peer::serve_castles(
                Path::new(&storage),
                0,
                vec![
                    (Default::default(), TOKEN.into()),
                    ("research".parse().unwrap(), research_token()),
                ],
            )
            .unwrap();
        } else {
            serve(Path::new(&storage), 0, TOKEN.into()).unwrap();
        }
    }
}

struct Server(Child, String);
impl Server {
    fn start(storage: &Path) -> Self {
        Self::configured(storage, false)
    }
    fn configured(storage: &Path, named: bool) -> Self {
        let mut command = Command::new(std::env::current_exe().unwrap());
        if named {
            command.env("COVEN_TEST_NAMED", "1");
        }
        let child = command
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
            if let Some(addr) = line.trim().strip_prefix(if named {
                "Serving Castle demo, research on "
            } else {
                "Serving Castle demo on "
            }) {
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

/// Compare committed rows, including pending operation IDs and payloads, without
/// exposing local configuration or credential paths in assertion diagnostics.
fn profile_rows(state: &Path) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    let db = rusqlite::Connection::open_with_flags(
        state.join("client.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    ["settings", "baseline", "pending", "conflicts", "incoming"]
        .map(|table| {
            let mut query = db
                .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
                .unwrap();
            let columns = query.column_count();
            query
                .query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        })
        .into()
}

#[test]
fn profile_refusal_preserves_both_peers_and_conflicts() {
    use coven_host::Client;
    use std::fs;
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().canonicalize().unwrap();
    let root = base.join("castle");
    let note = root.join("Castles/demo/files/note");
    let key = "files/note";
    let homes = [base.join("a"), base.join("b")];
    let states = homes.each_ref().map(|home| home.join("state"));
    let servers = [
        Server::start(&base.join("peer-a")),
        Server::start(&base.join("peer-b")),
    ];
    let mut peers = servers.each_ref().map(Server::peer);
    let legacy = states.each_ref().map(|state| Client::new(state));
    for i in 0..2 {
        legacy[i].initialize(&root).unwrap();
        let token = states[i].join("test-token");
        fs::write(&token, TOKEN).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
        }
        legacy[i].connect(&servers[i].1, &token).unwrap();
        fs::write(&note, b"baseline").unwrap();
        assert_eq!(legacy[i].sync().unwrap()["phase"], "synced");
        peers[i].inventory().unwrap();
        peers[i].upload(key, &[i as u8], 1, None).unwrap();
    }
    assert_ne!(peers[0].identity, peers[1].identity);
    fs::write(&note, b"offline change").unwrap();
    for client in &legacy {
        assert_eq!(client.sync().unwrap()["phase"], "needs attention");
        assert_eq!(client.status().unwrap()["conflicts"][0]["path"], key);
    }
    let profile = |i: usize| Client::for_profile(&states[i], &homes[i]);
    let owner = profile(0);
    owner.initialize(&root).unwrap();
    let alias = base.join("castle-alias");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    #[cfg(windows)]
    assert!(
        Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&alias)
            .arg(&root)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(alias.canonicalize().unwrap(), root.canonicalize().unwrap());
    let snapshot = || {
        let profiles = states.each_ref().map(|state| {
            let mut copies = fs::read_dir(state.join("recovery"))
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    (entry.file_name(), fs::read(entry.path()).unwrap())
                })
                .collect::<Vec<_>>();
            copies.sort();
            assert!(copies.len() >= 2);
            (profile_rows(state), copies)
        });
        (
            profiles,
            fs::read(&note).unwrap(),
            fs::read(root.join(".coven-profile")).unwrap(),
        )
    };
    let before = snapshot();
    let refuse = || {
        let competitor = profile(1);
        assert!(
            competitor
                .initialize(&root)
                .unwrap_err()
                .to_string()
                .contains("different daemon profile")
        );
        assert!(
            before == snapshot(),
            "canonical refusal changed existing state"
        );
        let error = competitor.initialize(&alias).unwrap_err();
        #[cfg(unix)]
        assert!(
            matches!(
                error
                    .downcast_ref::<std::io::Error>()
                    .and_then(|e| e.raw_os_error()),
                Some(libc::ENOTDIR | libc::ELOOP)
            ),
            "unexpected alias refusal: {error}"
        );
        #[cfg(windows)]
        assert!(
            error
                .to_string()
                .contains("reparse points and junctions are unsafe"),
            "unexpected alias refusal: {error}"
        );
        assert!(before == snapshot(), "alias refusal changed existing state");
    };
    refuse();
    drop(owner);
    refuse();
    let reopened = profile(0);
    reopened.resolve(key, "local", None).unwrap();
    assert_eq!(reopened.sync().unwrap()["phase"], "synced");
    for (i, peer) in peers.iter_mut().enumerate() {
        let item = peer.inventory().unwrap().remove(key).unwrap();
        let bytes = peer.download(key, &item).unwrap();
        if i == 0 {
            assert_eq!(bytes, b"offline change");
        } else {
            assert_eq!(bytes, [1]);
        }
    }
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

fn research_token() -> String {
    format!("{TOKEN}-research")
}

#[test]
fn castle_credentials_quotas_and_retry_receipts_are_independent_on_one_listener() {
    use coven_host::{CastleId, MAX_FILES, digest};
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().canonicalize().unwrap().join("peer");
    let research: CastleId = "research".parse().unwrap();
    let demo = CastleId::default();
    let server = Server::configured(&storage, true);
    let path = "files/note.md";
    let op = "12341234123412341234123412341234";
    let peers = |server: &Server| {
        [
            server.peer(),
            Peer::for_castle(&research, &server.1, research_token(), None).unwrap(),
        ]
    };
    let mut clients = peers(&server);
    let bytes: [&[u8]; 2] = [b"demo content", b"research content"];
    for (client, data) in clients.iter_mut().zip(bytes) {
        assert!(client.inventory().unwrap().is_empty());
        client.upload(path, data, 0, Some(op)).unwrap();
    }
    let identities = clients.each_ref().map(|p| p.identity.clone());
    assert_ne!(identities[0], identities[1]);
    for (castle, token, index) in [
        (&research, TOKEN.to_owned(), 1),
        (&demo, research_token(), 0),
    ] {
        let mut wrong = Peer::for_castle(castle, &server.1, token, None).unwrap();
        let item = clients[index].inventory().unwrap().remove(path).unwrap();
        // A fresh reader has only its credential. A stale identity pin cannot mask a routing leak.
        assert!(wrong.inventory().unwrap_err().to_string().contains("404"));
        assert!(
            wrong
                .download(path, &item)
                .unwrap_err()
                .to_string()
                .contains("404")
        );
        wrong.identity = identities[index].clone();
        assert!(wrong.upload(path, b"wrong", 1, None).is_err());
        assert!(wrong.delete(path, 1, op).is_err());
    }
    drop(server);
    let server = Server::configured(&storage, true);
    let mut clients = peers(&server);
    for (index, (client, data)) in clients.iter_mut().zip(bytes).enumerate() {
        let item = client.inventory().unwrap().remove(path).unwrap();
        assert_eq!(client.identity, identities[index]);
        client.upload(path, data, 0, Some(op)).unwrap();
        assert_eq!(item.revision, 1);
        assert_eq!(client.download(path, &item).unwrap(), data);
    }
    // Fill the demo index up to its real file limit in a single fixture transaction.
    let mut db = rusqlite::Connection::open(storage.join("host.sqlite")).unwrap();
    let tx = db.transaction().unwrap();
    let sha = digest(b"");
    for i in 1..MAX_FILES {
        tx.execute(
            "INSERT INTO files VALUES (?, 1, ?, x'')",
            (format!("files/filler-{i}"), &sha),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let extra = "files/extra";
    let rejected = clients[0].upload(extra, b"extra", 0, None).unwrap_err();
    assert!(rejected.to_string().contains("413"));
    clients[1].upload(extra, b"extra", 0, None).unwrap();
    assert_eq!(clients[0].inventory().unwrap().len(), MAX_FILES);
    let item = clients[1].inventory().unwrap().remove(extra).unwrap();
    assert_eq!(clients[1].download(extra, &item).unwrap(), b"extra");
}

#[test]
fn duplicate_registration_fails_before_creating_peer_storage() {
    let temp = tempfile::tempdir().unwrap();
    let storage = temp.path().join("unused");
    for (name, token) in [("demo", research_token()), ("research", TOKEN.into())] {
        let result = coven_host::peer::serve_castles(
            &storage,
            0,
            vec![
                (Default::default(), TOKEN.into()),
                (name.parse().unwrap(), token),
            ],
        );
        assert!(result.is_err());
        assert!(!storage.exists());
    }
}
