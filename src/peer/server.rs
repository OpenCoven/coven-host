//! Loopback peer service. The on-disk schema and retry receipt are shared with the Python host.
use std::{
    fs::OpenOptions,
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

use super::{Inventory, Item, PREFIX, Row, TIMEOUT, lower_hex};
use crate::{MAX_FILE, MAX_FILES, MAX_TOTAL, digest, validate_path};

struct Host {
    db: Mutex<Connection>,
    id: String,
    authorization: String,
}

impl Host {
    fn open(storage: &Path, token: String) -> Result<Self> {
        let db = Connection::open_with_flags(
            storage.join("host.sqlite"),
            OpenFlags::default() | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        db.busy_timeout(TIMEOUT)?;
        db.execute_batch("PRAGMA journal_mode=DELETE;
            PRAGMA synchronous=FULL;
            CREATE TABLE IF NOT EXISTS instance (slot INTEGER PRIMARY KEY, id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY, revision INTEGER NOT NULL, sha256 TEXT NOT NULL, bytes BLOB NOT NULL);
            CREATE TABLE IF NOT EXISTS receipt (
                slot INTEGER PRIMARY KEY, operation TEXT NOT NULL, path TEXT NOT NULL,
                expected INTEGER NOT NULL, sha256 TEXT NOT NULL);")?;
        db.execute(
            "INSERT OR IGNORE INTO instance VALUES (1, ?)",
            [Uuid::new_v4().simple().to_string()],
        )?;
        let id: String = db.query_row("SELECT id FROM instance WHERE slot=1", [], |r| r.get(0))?;
        ensure!(lower_hex(&id, 32), "invalid host identity in storage");
        Ok(Self {
            db: Mutex::new(db),
            id,
            authorization: format!("Bearer {token}"),
        })
    }

    fn get(&self, path: Option<&str>) -> Result<Response> {
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
            })?,
        ))
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
        if current != upload.expected {
            return Ok(error(412, "remote revision changed; sync again"));
        }
        if (current == 0 && count >= MAX_FILES) || total > MAX_TOTAL as i64 {
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

async fn handle(State(host): State<Arc<Host>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let headers = &parts.headers;
    let auth = single(headers, "authorization").unwrap_or("").as_bytes();
    if !bool::from(auth.ct_eq(host.authorization.as_bytes())) {
        return error(401, "credentials rejected; check the protected token file");
    }
    let put = parts.method == Method::PUT;
    if parts.method != Method::GET && !put {
        return error(405, "unsupported method");
    }
    if (put || headers.contains_key("x-host-id"))
        && single(headers, "x-host-id") != Some(host.id.as_str())
    {
        return error(
            409,
            "host identity mismatch; check your tunnel and server storage",
        );
    }
    let route = parts.uri.path_and_query().map(|p| p.as_str()).unwrap_or("");
    let path = if route == PREFIX && !put {
        None
    } else {
        let Some(path) = route.strip_prefix(PREFIX).and_then(|s| s.strip_prefix('/')) else {
            return error(404, "unknown Castle or route");
        };
        if validate_path(path).is_err() {
            return error(400, "unsupported Castle path");
        }
        Some(path)
    };
    let result = if put {
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
        host.get(path)
    };
    result.unwrap_or_else(|_| error(500, "host storage error"))
}

/// Serve one Castle on IPv4 loopback, retaining the Python-compatible storage lock until exit.
pub fn serve(storage: &Path, port: u16, token: String) -> Result<()> {
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
    let host = Arc::new(Host::open(storage, token)?);
    let service = TowerToHyperService::new(Router::new().fallback(handle).with_state(host));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
        println!("Serving Castle demo on {}", listener.local_addr()?);
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
