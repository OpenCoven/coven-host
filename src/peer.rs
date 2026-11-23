//! Authenticated client for the single-Castle HTTP protocol.
use std::{collections::BTreeMap, fmt, io::Read, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use reqwest::{
    Method, Url,
    blocking::Client,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{MAX_FILE, MAX_FILES, MAX_TOTAL, digest, validate_path};

mod server;
pub use server::serve;

const PREFIX: &str = "/v1/castles/demo/files";
const TIMEOUT: Duration = Duration::from_secs(5);
const INVENTORY_LIMIT: usize = 1024 * 1024;
const REVISION_LIMIT: i64 = 1_000_000_000_000_000_000;

fn lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn endpoint(url: &str) -> Result<()> {
    const MESSAGE: &str =
        "use verified https:// or http://127.0.0.1:<port> (local/verified SSH tunnel)";
    let (scheme, rest) = url.split_once("://").context(MESSAGE)?;
    let authority = rest.strip_suffix('/').unwrap_or(rest);
    ensure!(
        !authority.is_empty()
            && authority
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".:[]-".contains(&b)),
        MESSAGE
    );
    let parsed = Url::parse(url).map_err(|_| anyhow::anyhow!(MESSAGE))?;
    ensure!(
        parsed.host().is_some()
            && parsed.port() != Some(0)
            && (scheme == "https"
                || (scheme == "http" && authority.split(':').next() == Some("127.0.0.1"))),
        MESSAGE
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Item {
    pub revision: i64,
    pub sha256: String,
    pub size: usize,
}

#[derive(Deserialize, Serialize)]
struct Row {
    path: String,
    #[serde(flatten)]
    item: Item,
}

#[derive(Deserialize, Serialize)]
struct Inventory {
    host: String,
    files: Vec<Row>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deleted: Option<BTreeMap<String, i64>>,
}

#[derive(Debug)]
struct Offline;

impl fmt::Display for Offline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot contact host; check tunnel, service or TLS")
    }
}
impl std::error::Error for Offline {}

#[derive(Debug)]
struct Rejected(u16);

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self.0 {
            400 => "invalid request or unsupported path",
            401 => "check the token file",
            404 => "file or Castle not found; retry reconciliation",
            409 => "check host identity or operation ID",
            412 => "remote revision changed; sync again",
            413 => "upload exceeds Castle limits",
            422 => "file/directory collision; move the unsynchronized local entry",
            _ => "check server and tunnel",
        };
        write!(f, "host HTTP {}: {detail}", self.0)
    }
}
impl std::error::Error for Rejected {}

pub fn is_offline(error: &anyhow::Error) -> bool {
    error.is::<Offline>()
}
pub fn is_precondition(error: &anyhow::Error) -> bool {
    matches!(error.downcast_ref::<Rejected>(), Some(Rejected(412)))
}
pub fn is_write_rejected(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<Rejected>(),
        Some(Rejected(412 | 413 | 422))
    )
}

pub struct Peer {
    client: Client,
    base: String,
    pub identity: Option<String>,
    pub(crate) deleted: Option<BTreeMap<String, i64>>,
}

impl Peer {
    pub fn new(url: &str, token: String, identity: Option<String>) -> Result<Self> {
        endpoint(url)?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| anyhow::anyhow!("invalid token file format"))?;
        authorization.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(reqwest::header::AUTHORIZATION, authorization);
        let client = Client::builder()
            .default_headers(headers)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self {
            client,
            base: format!("{}{PREFIX}", url.trim_end_matches('/')),
            identity,
            deleted: None,
        })
    }

    fn request(
        &self,
        method: Method,
        path: Option<&str>,
        headers: HeaderMap,
        body: Option<&[u8]>,
        limit: usize,
    ) -> Result<(Vec<u8>, Option<String>)> {
        let url = match path {
            Some(path) => {
                validate_path(path)?;
                format!("{}/{path}", self.base)
            }
            None => self.base.clone(),
        };
        let mut request = self.client.request(method, url).headers(headers);
        if let Some(pin) = &self.identity {
            request = request.header("X-Host-ID", pin);
        }
        if let Some(body) = body {
            request = request.body(body.to_vec());
        }
        let response = request.send().map_err(|_| Offline)?;
        if response.status() != 200 {
            return Err(Rejected(response.status().as_u16()).into());
        }
        let mut revisions = response.headers().get_all("X-Revision").iter();
        let revision = revisions
            .next()
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        ensure!(revisions.next().is_none(), "duplicate host revision");
        let mut data = Vec::new();
        response
            .take(limit as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|_| Offline)?;
        ensure!(data.len() <= limit, "host response exceeds its size limit");
        Ok((data, revision))
    }

    pub fn inventory(&mut self) -> Result<BTreeMap<String, Item>> {
        let mut headers = HeaderMap::new();
        headers.insert("x-coven-deletions", HeaderValue::from_static("1"));
        let (body, _) = self.request(Method::GET, None, headers, None, INVENTORY_LIMIT * 3)?;
        let Inventory {
            host,
            files,
            deleted,
        } = serde_json::from_slice(&body).context("invalid host inventory")?;
        ensure!(
            lower_hex(&host, 32) && files.len() <= MAX_FILES,
            "invalid host inventory"
        );
        ensure!(
            self.identity.as_ref().is_none_or(|pin| pin == &host),
            "host identity changed; check the configured host and its persistent storage"
        );
        let mut result = BTreeMap::new();
        let mut total = 0;
        for Row { path, item } in files {
            validate_path(&path)?;
            ensure!(
                (1..REVISION_LIMIT).contains(&item.revision)
                    && lower_hex(&item.sha256, 64)
                    && item.size <= MAX_FILE,
                "invalid host file metadata"
            );
            total += item.size;
            ensure!(
                total <= MAX_TOTAL,
                "host inventory exceeds Castle storage limit"
            );
            ensure!(
                result.insert(path, item).is_none(),
                "duplicate path in host inventory"
            );
        }
        if let Some(entries) = &deleted {
            ensure!(entries.len() <= 2000, "invalid deletion inventory");
            for (path, rev) in entries {
                validate_path(path)?;
                ensure!(
                    (2..REVISION_LIMIT).contains(rev) && !result.contains_key(path),
                    "invalid deleted path metadata"
                );
            }
        }
        self.deleted = deleted;
        self.identity = Some(host);
        Ok(result)
    }

    pub fn download(&self, path: &str, item: &Item) -> Result<Vec<u8>> {
        let (data, revision) =
            self.request(Method::GET, Some(path), HeaderMap::new(), None, MAX_FILE)?;
        ensure!(
            revision == Some(item.revision.to_string())
                && data.len() == item.size
                && digest(&data) == item.sha256,
            "{path}: host file changed or transfer failed verification; sync again"
        );
        Ok(data)
    }

    pub fn delete(&self, path: &str, expected: i64, operation: &str) -> Result<()> {
        ensure!(
            (1..REVISION_LIMIT).contains(&expected),
            "invalid host revision"
        );
        ensure!(lower_hex(operation, 32), "invalid operation ID");
        let mut headers = HeaderMap::new();
        headers.insert("if-match", format!("\"{expected}\"").parse()?);
        headers.insert("x-sync-operation", operation.parse()?);
        let (body, _) = self.request(Method::DELETE, Some(path), headers, None, INVENTORY_LIMIT)?;
        let ack: serde_json::Value = serde_json::from_slice(&body)?;
        ensure!(
            ack == json!({"revision": expected + 1, "deleted": true}),
            "invalid deletion acknowledgement; retry reconciliation"
        );
        Ok(())
    }

    pub fn upload(
        &self,
        path: &str,
        data: &[u8],
        expected: i64,
        operation: Option<&str>,
    ) -> Result<()> {
        validate_path(path)?;
        ensure!(data.len() <= MAX_FILE, "{path}: exceeds 10 MiB limit");
        ensure!(
            (0..REVISION_LIMIT).contains(&expected),
            "invalid host revision"
        );
        let sha = digest(data);
        let mut headers = HeaderMap::new();
        headers.insert("if-match", format!("\"{expected}\"").parse()?);
        headers.insert("x-content-sha256", sha.parse()?);
        headers.insert("content-length", data.len().to_string().parse()?);
        if let Some(op) = operation {
            ensure!(lower_hex(op, 32), "invalid operation ID");
            headers.insert("x-sync-operation", op.parse()?);
        }
        let (body, _) = self.request(
            Method::PUT,
            Some(path),
            headers,
            Some(data),
            INVENTORY_LIMIT,
        )?;
        let ack: serde_json::Value = serde_json::from_slice(&body)
            .context("invalid upload acknowledgement; retry reconciliation")?;
        if ack != json!({ "revision": expected + 1, "sha256": sha }) {
            bail!("invalid upload acknowledgement; retry reconciliation");
        }
        Ok(())
    }
}
