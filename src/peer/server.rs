//! Loopback peer service. The on-disk schema and retry receipt are shared with the Python host.
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Write,
    net::Ipv4Addr,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use fs2::FileExt;
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use super::{Inventory, Item, Row, TIMEOUT, lower_hex};
use crate::{CastleId, MAX_FILE, MAX_FILES, MAX_TOTAL, digest, validate_path};

struct Host {
    route: String,
    db: Mutex<Connection>,
    id: String,
    authorization: String,
}

impl Host {
    fn open(storage: &Path, castle: &CastleId, token: String) -> Result<Self> {
        let mut db = Connection::open_with_flags(
            storage.join("host.sqlite"),
            OpenFlags::default() | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(TIMEOUT)?;
        let has_scope: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='castle_scope')",
            [],
            |row| row.get(0),
        )?;
        if has_scope {
            let id: String =
                db.query_row("SELECT id FROM castle_scope WHERE slot=1", [], |row| {
                    row.get(0)
                })?;
            ensure!(
                id == castle.as_str(),
                "peer database belongs to a different Castle; restore the correct database"
            );
        } else if castle.as_str() != "demo" {
            let existing: bool =
                db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema)", [], |row| {
                    row.get(0)
                })?;
            ensure!(
                !existing,
                "unscoped peer database belongs to demo; restore it to the original storage"
            );
        }
        db.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=FULL;")?;
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS castle_scope (slot INTEGER PRIMARY KEY, id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS instance (slot INTEGER PRIMARY KEY, id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY, revision INTEGER NOT NULL, sha256 TEXT NOT NULL, bytes BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS deleted_versions (
                path TEXT NOT NULL, revision INTEGER NOT NULL, sha256 TEXT NOT NULL,
                bytes BLOB NOT NULL, operation TEXT NOT NULL UNIQUE, PRIMARY KEY(path, revision));
            CREATE TABLE IF NOT EXISTS receipt (
                slot INTEGER PRIMARY KEY, operation TEXT NOT NULL, path TEXT NOT NULL,
                expected INTEGER NOT NULL, sha256 TEXT NOT NULL);")?;
        tx.execute(
            "INSERT OR IGNORE INTO castle_scope VALUES (1, ?)",
            [castle.as_str()],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO instance VALUES (1, ?)",
            [Uuid::new_v4().simple().to_string()],
        )?;
        let id: String = tx.query_row("SELECT id FROM instance WHERE slot=1", [], |r| r.get(0))?;
        ensure!(lower_hex(&id, 32), "invalid host identity in storage");
        tx.commit()?;
        Ok(Self {
            route: castle.route(),
            db: Mutex::new(db),
            id,
            authorization: format!("Bearer {token}"),
        })
    }

    fn get(&self, path: Option<&str>, include_deleted: bool) -> Result<Response> {
        let db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("host database lock failed"))?;
        if let Some(path) = path {
            let row: Option<(i64, Vec<u8>)> = db
                .query_row(
                    "SELECT revision, bytes FROM files WHERE path=?",
                    [path],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            return Ok(match row {
                Some((revision, bytes)) => (
                    [
                        (
                            header::CONTENT_TYPE.as_str(),
                            "application/octet-stream".to_owned(),
                        ),
                        (header::CACHE_CONTROL.as_str(), "no-store".to_owned()),
                        ("x-revision", revision.to_string()),
                    ],
                    bytes,
                )
                    .into_response(),
                None => error(404, "file not found"),
            });
        }
        let mut q =
            db.prepare("SELECT path, revision, sha256, length(bytes) FROM files ORDER BY path")?;
        let files = q
            .query_map([], |r| {
                Ok(Row {
                    path: r.get(0)?,
                    item: Item {
                        revision: r.get(1)?,
                        sha256: r.get(2)?,
                        size: r.get::<_, u32>(3)? as usize,
                    },
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(json_reply(
            StatusCode::OK,
            serde_json::to_value(Inventory {
                host: self.id.clone(),
                files,
                deleted: if include_deleted {
                    let mut q = db.prepare("SELECT d.path, max(d.revision) FROM deleted_versions d WHERE NOT EXISTS (SELECT 1 FROM files f WHERE f.path=d.path) GROUP BY d.path")?;
                    Some(
                        q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                            .collect::<rusqlite::Result<_>>()?,
                    )
                } else {
                    None
                },
            })?,
        ))
    }

    fn delete(&self, path: &str, expected: i64, operation: &str) -> Result<Response> {
        let mut db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("host database lock failed"))?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision = expected + 1;
        let ack = || {
            json_reply(
                StatusCode::OK,
                json!({"revision": revision, "deleted": true}),
            )
        };
        if let Some((prior_path, prior_revision)) = tx
            .query_row(
                "SELECT path, revision FROM deleted_versions WHERE operation=?",
                [operation],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
        {
            return Ok(if prior_path == path && prior_revision == revision {
                ack()
            } else {
                error(409, "operation ID already used for a different deletion")
            });
        }
        let size: Option<i64> = tx
            .query_row(
                "SELECT length(bytes) FROM files WHERE path=? AND revision=?",
                params![path, expected],
                |r| r.get(0),
            )
            .optional()?;
        let Some(size) = size else {
            return Ok(error(412, "remote revision changed; sync again"));
        };
        let (count, total): (i64, i64) = tx.query_row(
            "SELECT count(*), coalesce(sum(length(bytes)),0) FROM deleted_versions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if count >= 2000 || total + size > MAX_TOTAL as i64 {
            return Ok(error(413, "peer deletion recovery archive is full"));
        }
        tx.execute(
            "INSERT INTO deleted_versions SELECT path, ?, sha256, bytes, ? FROM files WHERE path=?",
            params![revision, operation, path],
        )?;
        tx.execute("DELETE FROM files WHERE path=?", [path])?;
        tx.commit()?;
        Ok(ack())
    }

    fn put(&self, path: &str, upload: &Upload, data: &[u8]) -> Result<Response> {
        let mut db = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("host database lock failed"))?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(op) = &upload.operation {
            let previous: Option<(String, i64, String)> = tx
                .query_row(
                    "SELECT path,expected,sha256 FROM receipt WHERE slot=1 AND operation=?",
                    [op],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            if let Some((name, rev, hash)) = previous {
                if name != path || rev != upload.expected || hash != upload.sha {
                    return Ok(error(
                        409,
                        "operation ID already used for a different write",
                    ));
                }
                tx.commit()?;
                return Ok(upload.ack());
            }
        }

        let mut current = 0;
        let mut count = 0;
        let mut total: i64 = data.len() as i64;
        {
            let mut q = tx.prepare("SELECT path, revision, length(bytes) FROM files")?;
            let mut rows = q.query([])?;
            while let Some(row) = rows.next()? {
                let existing: String = row.get(0)?;
                count += 1;
                if existing == path {
                    current = row.get(1)?;
                } else {
                    let nested = |a: &str, parent: &str| {
                        a.strip_prefix(parent)
                            .is_some_and(|suffix| suffix.starts_with('/'))
                    };
                    if nested(path, &existing) || nested(&existing, path) {
                        return Ok(error(422, "file/directory name collision"));
                    }
                    total += row.get::<_, i64>(2)?;
                }
            }
        }
        let creating = current == 0;
        if creating {
            current = tx.query_row(
                "SELECT coalesce(max(revision),0) FROM deleted_versions WHERE path=?",
                [path],
                |r| r.get(0),
            )?;
        }
        if current != upload.expected {
            return Ok(error(412, "remote revision changed; sync again"));
        }
        if (creating && count >= MAX_FILES) || total > MAX_TOTAL as i64 {
            return Ok(error(413, "Castle exceeds 100 MiB or 1000 files"));
        }
        tx.execute(
            "INSERT OR REPLACE INTO files VALUES (?, ?, ?, ?)",
            params![path, current + 1, upload.sha, data],
        )?;
        if let Some(op) = &upload.operation {
            tx.execute(
                "INSERT OR REPLACE INTO receipt VALUES (1, ?, ?, ?, ?)",
                params![op, path, current, upload.sha],
            )?;
        }
        tx.commit()?;
        Ok(upload.ack())
    }
}

fn json_reply(status: StatusCode, value: Value) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(value),
    )
        .into_response()
}

fn error(status: u16, message: &str) -> Response {
    json_reply(
        StatusCode::from_u16(status).expect("static status"),
        json!({"error": message}),
    )
}

fn single<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    let mut values = headers.get_all(key).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

struct Upload {
    size: usize,
    expected: i64,
    sha: String,
    operation: Option<String>,
}

impl Upload {
    fn from_headers(h: &HeaderMap) -> Option<Self> {
        if h.contains_key(header::TRANSFER_ENCODING) {
            return None;
        }
        let size = single(h, "content-length")?;
        let revision = single(h, "if-match")?
            .strip_prefix('"')?
            .strip_suffix('"')?;
        let sha = single(h, "x-content-sha256")?;
        let digits =
            |s: &str, max| !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_digit());
        if !digits(size, 12) || !digits(revision, 18) || !lower_hex(sha, 64) {
            return None;
        }
        let operation = if h.contains_key("x-sync-operation") {
            let op = single(h, "x-sync-operation")?;
            if !lower_hex(op, 32) {
                return None;
            }
            Some(op.to_owned())
        } else {
            None
        };
        Some(Self {
            size: size.parse().ok()?,
            expected: revision.parse().ok()?,
            sha: sha.to_owned(),
            operation,
        })
    }

    fn ack(&self) -> Response {
        json_reply(
            StatusCode::OK,
            json!({"revision": self.expected + 1, "sha256": self.sha}),
        )
    }
}

async fn handle(State(hosts): State<Arc<Vec<Host>>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let headers = &parts.headers;
    let auth = single(headers, "authorization").unwrap_or("").as_bytes();
    let Some(host) = hosts
        .iter()
        .find(|host| bool::from(auth.ct_eq(host.authorization.as_bytes())))
    else {
        return error(401, "credentials rejected; check the protected token file");
    };
    let put = parts.method == Method::PUT;
    let delete = parts.method == Method::DELETE;
    if parts.method != Method::GET && !put && !delete {
        return error(405, "unsupported method");
    }
    if (put || delete || headers.contains_key("x-host-id"))
        && single(headers, "x-host-id") != Some(host.id.as_str())
    {
        return error(
            409,
            "host identity mismatch; check your tunnel and server storage",
        );
    }
    let route = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("");
    let path = if route == host.route && !put && !delete {
        None
    } else {
        let Some(path) = route
            .strip_prefix(&host.route)
            .and_then(|s| s.strip_prefix('/'))
        else {
            return error(404, "unknown Castle or route");
        };
        if validate_path(path).is_err() {
            return error(400, "unsupported Castle path");
        }
        Some(path)
    };
    let result = if delete {
        let expected = single(headers, "if-match")
            .and_then(|value| value.strip_prefix('"')?.strip_suffix('"'))
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|v| (1..super::REVISION_LIMIT).contains(v));
        let op = single(headers, "x-sync-operation").filter(|s| lower_hex(s, 32));
        let (Some(expected), Some(op)) = (expected, op) else {
            return error(400, "expected revision and deletion operation ID required");
        };
        host.delete(path.expect("DELETE has file path"), expected, op)
    } else if put {
        let Some(upload) = Upload::from_headers(headers) else {
            return error(400, "invalid length, revision, hash or operation");
        };
        if upload.size > MAX_FILE {
            return error(413, "file exceeds 10 MiB limit");
        }
        let data = match tokio::time::timeout(TIMEOUT, to_bytes(Body::new(body), upload.size)).await
        {
            Err(_) => return error(408, "upload incomplete"),
            Ok(Err(_)) => return error(400, "upload incomplete or hash does not match"),
            Ok(Ok(data)) => data,
        };
        if data.len() != upload.size || digest(&data) != upload.sha {
            return error(400, "upload incomplete or hash does not match");
        }
        host.put(path.expect("PUT has file path"), &upload, &data)
    } else {
        host.get(path, single(headers, "x-coven-deletions") == Some("1"))
    };
    result.unwrap_or_else(|_| error(500, "host storage error"))
}

/// Serve the original demo Castle, preserving legacy storage and protocol.
pub fn serve(storage: &Path, port: u16, token: String) -> Result<()> {
    serve_castles(storage, port, vec![(CastleId::default(), token)])
}

fn lock_storage(storage: &Path) -> Result<File> {
    crate::folder::private_dir(storage)?;
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let lock = options.open(storage.join("client.lock"))?;
    ensure!(
        lock.metadata()?.is_file(),
        "expected regular service lock file"
    );
    FileExt::try_lock_exclusive(&lock).context("host storage is already in use")?;
    Ok(lock)
}

/// Serve explicitly configured Castles on one loopback endpoint with separate credentials.
pub fn serve_castles(storage: &Path, port: u16, castles: Vec<(CastleId, String)>) -> Result<()> {
    ensure!(!castles.is_empty(), "configure at least one Castle");
    let mut ids = BTreeSet::new();
    let mut tokens = BTreeSet::new();
    for (id, token) in &castles {
        ensure!(ids.insert(id), "duplicate Castle ID: {id}");
        ensure!(
            tokens.insert(token),
            "each Castle requires a different token"
        );
    }
    let names = castles
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut locks = vec![lock_storage(storage)?];
    let mut hosts = Vec::with_capacity(castles.len());
    for (castle, token) in castles {
        let path = castle.state_dir(storage);
        if path != storage {
            locks.push(lock_storage(&path)?);
        }
        hosts.push(Host::open(&path, &castle, token)?);
    }
    let host = Arc::new(hosts);
    let service = TowerToHyperService::new(Router::new().fallback(handle).with_state(host));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
        println!("Serving Castle {names} on {}", listener.local_addr()?);
        std::io::stdout().flush()?;
        let mut http = http1::Builder::new();
        http.keep_alive(false)
            .timer(TokioTimer::new())
            .header_read_timeout(TIMEOUT);
        loop {
            let (socket, _) = listener.accept().await?;
            // Match the reference's single active transfer and bound time spent on an abandoned reader.
            let connection = http.serve_connection(TokioIo::new(socket), service.clone());
            let _ = tokio::time::timeout(Duration::from_secs(15), connection).await;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misplaced_peer_storage_is_refused_before_serving_its_files() {
        let temp = tempfile::tempdir().unwrap();
        let storage = temp.path().canonicalize().unwrap();
        let demo = CastleId::default();
        let research = "research".parse().unwrap();
        drop(Host::open(&storage, &demo, Uuid::new_v4().to_string()).unwrap());
        let open_research = || Host::open(&storage, &research, Uuid::new_v4().to_string());
        assert!(
            open_research()
                .err()
                .unwrap()
                .to_string()
                .contains("different Castle")
        );
        let db = Connection::open(storage.join("host.sqlite")).unwrap();
        db.execute("DROP TABLE castle_scope", []).unwrap();
        drop(db);
        assert!(
            open_research()
                .err()
                .unwrap()
                .to_string()
                .contains("belongs to demo")
        );
        Host::open(&storage, &demo, Uuid::new_v4().to_string()).unwrap();
    }
}
