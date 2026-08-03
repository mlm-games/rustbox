//! rustbox online level-sharing backend.
//!
//! Cloudflare Worker (Rust wasm32) backed by D1 (metadata) + R2 (level
//! files). Reads are public and anonymous; writes are authorized by an
//! anonymous creator identity instead of a shared upload token:
//!
//! ```text
//! GET    /v1/levels                     list published levels
//! GET    /v1/levels/:id                 single level metadata
//! GET    /v1/levels/:id/data            raw level bytes (bincode+deflate)
//! GET    /v1/me                         creator identity + weekly quota
//! GET    /v1/me/levels                  the caller's published levels
//! POST   /v1/levels                     multipart upload (creator identity)
//! POST   /v1/levels/:id/report          report a level (per-IP rate limited)
//! POST   /v1/levels/:id/like            like a level (per-IP rate limited)
//! DELETE /v1/levels/:id                 delete (owner or admin token)
//! OPTIONS ...                           CORS preflight
//! ```
//!
//! ## Identity model
//!
//! The client generates a 32-byte secret on first run and shows it to the
//! player as a portable "recovery key" (`rbx1_` + URL-safe base64). It sends
//! that key as `Authorization: Bearer ...` plus a local-only device id as
//! `X-Rustbox-Device: <hex sha256>`. The server only ever stores
//! `owner_id = sha256(recovery_key bytes)`; leaking the D1 table leaks no
//! credentials. A player "logs in" on another device simply by pasting the
//! recovery key.
//!
//! The admin `UPLOAD_TOKEN` (a Worker secret) still works as an override for
//! delete, and for uploads it bypasses the quota (level gets `owner_id = ''`,
//! so it can only be deleted by the admin token afterwards). Old levels have
//! `owner_id = ''` and remain admin-only, exactly as before.
//!
//! ## Quota
//!
//! The weekly upload quota is exact accounting in D1 (`upload_events`), not a
//! windowed counter: 10/week per owner plus a looser 30/week per IP bucket
//! (IPv6 bucketed by /64) so shared networks aren't locked out. A slot is
//! claimed atomically before insert and released if the insert fails.

use std::collections::HashMap;
use std::net::IpAddr;

use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use wasm_bindgen::JsValue;
use worker::d1::D1Database;
use worker::{
    event, Cors, Env, FormEntry, Headers, Method, Request, Response, Result, RouteContext, Router,
};

use rustbox_format::api::{ApiError, LevelListResponse, LevelMeta, MeResponse, UploadMetadata, UploadResponse};
use rustbox_format::file::{decode_level, validate_level};
use rustbox_format::{API_VERSION, MAX_TAGS, MAX_UPLOAD_BYTES};

const DB: &str = "DB";
const BUCKET: &str = "LEVELS";
const LIST_LIMIT_MAX: u64 = 100;
const RATE_WINDOW_SECS: i64 = 300; // 5 minutes
const GENERAL_PER_WINDOW: u32 = 60;

const OWNER_UPLOADS_PER_WEEK: i64 = 10;
const IP_UPLOADS_PER_WEEK: i64 = 30;
const WEEK_SECS: i64 = 7 * 24 * 60 * 60;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    Router::new()
        .get_async("/v1/levels", list_levels)
        .get_async("/v1/levels/:id", get_level)
        .get_async("/v1/levels/:id/data", get_level_data)
        .get_async("/v1/me", handle_me)
        .get_async("/v1/me/levels", handle_my_levels)
        .post_async("/v1/levels", upload_level)
        .post_async("/v1/levels/:id/report", report_level)
        .post_async("/v1/levels/:id/like", like_level)
        .delete_async("/v1/levels/:id", delete_level)
        .options_async("/v1/levels", preflight)
        .options_async("/v1/levels/:id", preflight)
        .options_async("/v1/levels/:id/data", preflight)
        .options_async("/v1/me", preflight)
        .options_async("/v1/me/levels", preflight)
        .on_async("/", health)
        .run(req, env)
        .await
}

/// Internal route failure; converted to a JSON error response at the edge.
struct ApiFailure {
    status: u16,
    msg: String,
}

impl ApiFailure {
    fn new(status: u16, msg: impl Into<String>) -> Self {
        Self {
            status,
            msg: msg.into(),
        }
    }
    fn bad(msg: impl Into<String>) -> Self {
        Self::new(400, msg)
    }
    fn not_found(msg: impl Into<String>) -> Self {
        Self::new(404, msg)
    }
    fn too_many() -> Self {
        Self::new(429, "too many requests")
    }
}

impl From<worker::Error> for ApiFailure {
    fn from(e: worker::Error) -> Self {
        Self::new(500, e.to_string())
    }
}

impl From<anyhow::Error> for ApiFailure {
    fn from(e: anyhow::Error) -> Self {
        Self::new(400, e.to_string())
    }
}

type AppResult<T = Response> = std::result::Result<T, ApiFailure>;

/// Convert an `AppResult` into a `worker::Result<Response>`, replacing failures
/// with a CORS-wrapped JSON error body.
fn finish(env: &Env, result: AppResult) -> Result<Response> {
    match result {
        Ok(res) => Ok(res),
        Err(ApiFailure { status, msg }) => {
            let mut headers = Headers::new();
            cors(env).apply_headers(&mut headers)?;
            let mut res = Response::from_json(&ApiError { error: msg })?;
            res = res.with_headers(headers).with_status(status);
            Ok(res)
        }
    }
}

fn cors(env: &Env) -> Cors {
    let origins: Vec<String> = match env.var("ALLOWED_ORIGINS") {
        Ok(v) => v
            .to_string()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Err(_) => vec![
            "https://mlm-games.github.io".to_string(),
            "http://localhost:8080".to_string(),
        ],
    };
    Cors::new()
        .with_origins(origins)
        .with_methods([Method::Get, Method::Post, Method::Delete, Method::Options])
        .with_allowed_headers(["Content-Type", "X-Auth-Token", "Authorization", "X-Rustbox-Device"])
        .with_exposed_headers(["Content-Type", "Location"])
        .with_max_age(86_400)
}

fn json_ok<T: Serialize>(env: &Env, value: &T, status: u16) -> AppResult {
    let mut headers = Headers::new();
    cors(env).apply_headers(&mut headers)?;
    let mut res = Response::from_json(value)?;
    res = res.with_headers(headers).with_status(status);
    Ok(res)
}

fn empty_ok(env: &Env, status: u16) -> AppResult {
    let mut headers = Headers::new();
    cors(env).apply_headers(&mut headers)?;
    let mut res = Response::empty()?;
    res = res.with_headers(headers).with_status(status);
    Ok(res)
}

fn bytes_ok(env: &Env, bytes: Vec<u8>) -> AppResult {
    let mut headers = Headers::new();
    cors(env).apply_headers(&mut headers)?;
    headers.set("Content-Type", "application/octet-stream")?;
    let mut res = Response::from_bytes(bytes)?;
    res = res.with_headers(headers);
    Ok(res)
}

fn is_authorized(env: &Env, req: &Request) -> bool {
    let Ok(Some(given)) = req.headers().get("X-Auth-Token") else {
        return false;
    };
    env.secret("UPLOAD_TOKEN")
        .ok()
        .is_some_and(|token| token.to_string().trim() == given.trim())
}

fn client_ip(req: &Request) -> String {
    req.headers()
        .get("CF-Connecting-IP")
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Who is calling, after the identity headers parse. Never contains the raw
/// recovery key.
struct Caller {
    owner_id: String,
    device_id: String,
    ip_bucket: String,
}

/// `Authorization: Bearer rbx1_<url-safe base64 of 34 bytes>` -> the recovery
/// key, normalized (prefix stripped, whitespace removed).
fn bearer_recovery_key(req: &Request) -> Option<String> {
    let auth = req.headers().get("Authorization").ok().flatten()?;
    let raw = auth
        .strip_prefix("Bearer ")
        .or_else(|| auth.strip_prefix("bearer "))?
        .trim();
    normalize_recovery_key(raw)
}

fn normalize_recovery_key(raw: &str) -> Option<String> {
    let s: String = raw.trim().chars().filter(|c| !c.is_whitespace()).collect();
    let s = match s.strip_prefix("rbx1_") {
        Some(rest) => rest.to_string(),
        None => s,
    };
    let ok = (20..=64).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    ok.then_some(s)
}

/// What we store: sha256 of the 32 secret bytes, hex-encoded. The key carries
/// a 2-byte checksum (`sha256(secret)[..2]`) appended before base64; it's
/// verified here so a mistyped key gets a clean 400 instead of silently
/// becoming a different (and unowned) identity.
fn owner_id_of(recovery_key: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(recovery_key)
        .ok()?;
    if bytes.len() != 34 {
        return None;
    }
    let (secret, check) = bytes.split_at(32);
    if sha2::Sha256::digest(secret)[..2] != *check {
        return None;
    }
    Some(hex::encode(sha2::Sha256::digest(secret)))
}

/// `X-Rustbox-Device: <64 hex>`.
fn device_id(req: &Request) -> Option<String> {
    let d = req.headers().get("X-Rustbox-Device").ok().flatten()?;
    let d = d.trim().to_ascii_lowercase();
    (d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit())).then_some(d)
}

/// Bucket an IP for quota purposes: IPv4 as-is, IPv6 by its /64 prefix
/// (users often control an entire /64, so per-address limiting is pointless).
fn ip_bucket(ip: &str) -> String {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(v4)) => v4.to_string(),
        Ok(IpAddr::V6(v6)) => {
            let s = v6.segments();
            format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
        }
        Err(_) => "unknown".to_string(),
    }
}

fn extract_caller(req: &Request) -> AppResult<Caller> {
    let key =
        bearer_recovery_key(req).ok_or_else(|| ApiFailure::new(401, "missing creator key"))?;
    let owner_id = owner_id_of(&key).ok_or_else(|| ApiFailure::new(400, "invalid creator key"))?;
    let device_id = device_id(req).ok_or_else(|| ApiFailure::new(400, "missing X-Rustbox-Device"))?;
    Ok(Caller {
        owner_id,
        device_id,
        ip_bucket: ip_bucket(&client_ip(req)),
    })
}

async fn ensure_owner_and_device(db: &D1Database, caller: &Caller, now: i64) -> AppResult<()> {
    db.prepare(
        "INSERT INTO owners (owner_id, created_at, last_seen_at) VALUES (?, ?, ?) \
         ON CONFLICT(owner_id) DO UPDATE SET last_seen_at = excluded.last_seen_at",
    )
    .bind(&[
        JsValue::from_str(&caller.owner_id),
        JsValue::from(now as f64),
        JsValue::from(now as f64),
    ])?
    .run()
    .await?;
    db.prepare(
        "INSERT INTO devices (device_id, owner_id, created_at, last_seen_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(device_id) DO UPDATE SET owner_id = excluded.owner_id, last_seen_at = excluded.last_seen_at",
    )
    .bind(&[
        JsValue::from_str(&caller.device_id),
        JsValue::from_str(&caller.owner_id),
        JsValue::from(now as f64),
        JsValue::from(now as f64),
    ])?
    .run()
    .await?;
    Ok(())
}

#[derive(Deserialize, Default)]
struct QuotaRow {
    count: Option<i64>,
    oldest: Option<i64>,
}

async fn owner_quota(db: &D1Database, owner_id: &str, now: i64) -> AppResult<QuotaRow> {
    let since = now - WEEK_SECS;
    Ok(db
        .prepare(
            "SELECT COUNT(*) AS count, MIN(created_at) AS oldest \
             FROM upload_events WHERE owner_id = ? AND created_at > ?",
        )
        .bind(&[JsValue::from_str(owner_id), JsValue::from(since as f64)])?
        .first::<QuotaRow>(None)
        .await?
        .unwrap_or_default())
}

/// Atomically claim a weekly upload slot (both owner and IP caps) by inserting
/// an `upload_events` row. Returns `None` when a quota is exhausted.
async fn claim_upload_slot(db: &D1Database, caller: &Caller, now: i64) -> AppResult<Option<i64>> {
    let since = now - WEEK_SECS;
    let row = db
        .prepare(
            "INSERT INTO upload_events (owner_id, device_id, ip_bucket, created_at) \
             SELECT ?, ?, ?, ? \
             WHERE \
                (SELECT COUNT(*) FROM upload_events WHERE owner_id = ? AND created_at > ?) < ? \
                AND \
                (SELECT COUNT(*) FROM upload_events WHERE ip_bucket = ? AND created_at > ?) < ? \
             RETURNING id",
        )
        .bind(&[
            JsValue::from_str(&caller.owner_id),
            JsValue::from_str(&caller.device_id),
            JsValue::from_str(&caller.ip_bucket),
            JsValue::from(now as f64),
            JsValue::from_str(&caller.owner_id),
            JsValue::from(since as f64),
            JsValue::from(OWNER_UPLOADS_PER_WEEK as f64),
            JsValue::from_str(&caller.ip_bucket),
            JsValue::from(since as f64),
            JsValue::from(IP_UPLOADS_PER_WEEK as f64),
        ])?
        .first::<i64>(None)
        .await?;
    Ok(row)
}

async fn release_upload_slot(db: &D1Database, slot_id: i64) -> AppResult<()> {
    db.prepare("DELETE FROM upload_events WHERE id = ?")
        .bind(&[JsValue::from(slot_id as f64)])?
        .run()
        .await?;
    Ok(())
}

fn query_params(req: &Request) -> HashMap<String, String> {
    let Ok(url) = req.url() else {
        return HashMap::new();
    };
    url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// Sliding-window counter in D1 for low-stakes actions (report/like).
/// Returns Ok(()) if under the limit. Uploads use the exact `upload_events`
/// quota instead, not this.
async fn check_rate(env: &Env, bucket: &str, ip: &str, max: u32) -> AppResult<()> {
    let db = env.d1(DB)?;
    let window = Utc::now().timestamp() / RATE_WINDOW_SECS;
    let key = format!("{bucket}:{ip}:{window}");
    let row = db
        .prepare(
            "INSERT INTO rate_limits (key, count, window) VALUES (?, 1, ?) \
             ON CONFLICT(key) DO UPDATE SET count = count + 1 \
             RETURNING count",
        )
        .bind(&[JsValue::from(key.as_str()), JsValue::from(window)])?
        .first::<i64>(None)
        .await?;
    if let Some(count) = row
        && count as u32 > max
    {
        return Err(ApiFailure::too_many());
    }
    Ok(())
}

#[derive(Deserialize)]
struct LevelRow {
    id: i64,
    author: String,
    name: String,
    description: String,
    tags: String,
    format_version: i64,
    game_version: String,
    size_bytes: i64,
    sha256: String,
    likes: i64,
    plays: i64,
    #[allow(dead_code)] // deserialized from SELECT *, filtered in SQL
    status: String,
    #[serde(default)]
    #[allow(dead_code)] // used for delete ownership checks
    owner_id: String,
    created_at: String,
    updated_at: String,
}

impl LevelRow {
    fn into_meta(self) -> LevelMeta {
        let tags: Vec<String> = serde_json::from_str(&self.tags).unwrap_or_default();
        LevelMeta {
            id: self.id as u64,
            author: self.author,
            name: self.name,
            description: self.description,
            tags,
            format_version: self.format_version as u32,
            game_version: self.game_version,
            size_bytes: self.size_bytes as u64,
            sha256: self.sha256,
            likes: self.likes as u64,
            plays: self.plays as u64,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

async fn row_by_id(db: &D1Database, id: u64) -> AppResult<Option<LevelRow>> {
    let row = db
        .prepare("SELECT * FROM levels WHERE id = ? AND status != 'deleted'")
        .bind(&[JsValue::from(id as f64)])?
        .first::<LevelRow>(None)
        .await?;
    Ok(row)
}

async fn health(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(&ctx.env, json_ok(&ctx.env, &serde_json::json!({"ok": true, "service": "rustbox"}), 200))
}

/// The first call also lazily creates the owner/device rows (no registration endpoint needed).
async fn handle_me(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let db = ctx.env.d1(DB)?;
            let caller = extract_caller(&req)?;
            let now = Utc::now().timestamp();
            ensure_owner_and_device(&db, &caller, now).await?;
            let quota = owner_quota(&db, &caller.owner_id, now).await?;
            let used = quota.count.unwrap_or(0);
            let reset_at = quota.oldest.map(|t| t + WEEK_SECS);
            json_ok(
                &ctx.env,
                &MeResponse {
                    owner_id_short: caller.owner_id.chars().take(10).collect(),
                    uploads_used_this_week: used,
                    uploads_remaining_this_week: (OWNER_UPLOADS_PER_WEEK - used).max(0),
                    reset_at_unix: reset_at,
                },
                200,
            )
        }
        .await,
    )
}

async fn handle_my_levels(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let db = ctx.env.d1(DB)?;
            let caller = extract_caller(&req)?;
            ensure_owner_and_device(&db, &caller, Utc::now().timestamp()).await?;
            let rows = db
                .prepare(
                    "SELECT * FROM levels WHERE owner_id = ? AND status != 'deleted' \
                     ORDER BY created_at DESC",
                )
                .bind(&[JsValue::from_str(&caller.owner_id)])?
                .all()
                .await?
                .results::<LevelRow>()?;
            let total = rows.len() as u64;
            json_ok(
                &ctx.env,
                &LevelListResponse {
                    levels: rows.into_iter().map(LevelRow::into_meta).collect(),
                    total,
                },
                200,
            )
        }
        .await,
    )
}

async fn preflight(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(&ctx.env, empty_ok(&ctx.env, 204))
}

async fn list_levels(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let db = ctx.env.d1(DB)?;
            let q = req
                .headers()
                .get("X-Query")
                .ok()
                .flatten()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_lowercase());
            let query = query_params(&req);
            let limit = LIST_LIMIT_MAX.min(
                query
                    .get("limit")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(50),
            );
            let offset = query
                .get("offset")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);

            let (rows, total): (Vec<LevelRow>, i64) = match &q {
                Some(term) => {
                    let like = format!("%{term}%");
                    let rows = db
                        .prepare(
                            "SELECT * FROM levels WHERE status = 'published' \
                             AND (LOWER(name) LIKE ? OR LOWER(author) LIKE ?) \
                             ORDER BY created_at DESC LIMIT ? OFFSET ?",
                        )
                        .bind(&[
                            JsValue::from(like.as_str()),
                            JsValue::from(like.as_str()),
                            JsValue::from(limit as f64),
                            JsValue::from(offset as f64),
                        ])?
                        .all()
                        .await?
                        .results::<LevelRow>()?;
                    let total = db
                        .prepare(
                            "SELECT COUNT(*) AS n FROM levels WHERE status = 'published' \
                             AND (LOWER(name) LIKE ? OR LOWER(author) LIKE ?)",
                        )
                        .bind(&[JsValue::from(like.as_str()), JsValue::from(like.as_str())])?
                        .first::<i64>(Some("n"))
                        .await?
                        .unwrap_or(0);
                    (rows, total)
                }
                None => {
                    let rows = db
                        .prepare(
                            "SELECT * FROM levels WHERE status = 'published' \
                             ORDER BY created_at DESC LIMIT ? OFFSET ?",
                        )
                        .bind(&[JsValue::from(limit as f64), JsValue::from(offset as f64)])?
                        .all()
                        .await?
                        .results::<LevelRow>()?;
                    let total = db
                        .prepare("SELECT COUNT(*) AS n FROM levels WHERE status = 'published'")
                        .first::<i64>(Some("n"))
                        .await?
                        .unwrap_or(0);
                    (rows, total)
                }
            };

            json_ok(
                &ctx.env,
                &LevelListResponse {
                    levels: rows.into_iter().map(LevelRow::into_meta).collect(),
                    total: total as u64,
                },
                200,
            )
        }
        .await,
    )
}

async fn get_level(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let db = ctx.env.d1(DB)?;
            let id: u64 = ctx
                .param("id")
                .ok_or_else(|| ApiFailure::bad("bad level id"))?
                .parse()
                .map_err(|_| ApiFailure::bad("bad level id"))?;
            let Some(row) = row_by_id(&db, id).await? else {
                return Err(ApiFailure::not_found("level not found"));
            };
            json_ok(&ctx.env, &row.into_meta(), 200)
        }
        .await,
    )
}

async fn get_level_data(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let db = ctx.env.d1(DB)?;
            let bucket = ctx.env.bucket(BUCKET)?;
            let id: u64 = ctx
                .param("id")
                .ok_or_else(|| ApiFailure::bad("bad level id"))?
                .parse()
                .map_err(|_| ApiFailure::bad("bad level id"))?;
            let Some(row) = row_by_id(&db, id).await? else {
                return Err(ApiFailure::not_found("level not found"));
            };
            let key = format!("levels/{}.bin", row.id);
            let obj = bucket
                .get(key)
                .execute()
                .await?
                .ok_or_else(|| ApiFailure::not_found("level data missing"))?;
            let bytes = obj
                .body()
                .ok_or_else(|| ApiFailure::new(500, "level data missing body"))?
                .bytes()
                .await?;
            let _ = db
                .prepare("UPDATE levels SET plays = plays + 1 WHERE id = ?")
                .bind(&[JsValue::from(id as f64)])?
                .run()
                .await;
            bytes_ok(&ctx.env, bytes)
        }
        .await,
    )
}

async fn upload_level(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let caller = extract_caller(&req).ok();
            let is_admin = is_authorized(&ctx.env, &req);
            if caller.is_none() && !is_admin {
                return Err(ApiFailure::new(
                    401,
                    "missing or invalid creator key (or upload token)",
                ));
            }

            let form = req.form_data().await?;
            let meta_json = form
                .get_field("metadata")
                .ok_or_else(|| ApiFailure::bad("missing 'metadata' field"))?;
            let meta: UploadMetadata = serde_json::from_str(&meta_json)
                .map_err(|_| ApiFailure::bad("bad metadata JSON"))?;
            if meta.format_version != API_VERSION {
                return Err(ApiFailure::bad(format!(
                    "unsupported format version {}",
                    meta.format_version
                )));
            }
            if meta.name.trim().is_empty() || meta.name.chars().count() > 64 {
                return Err(ApiFailure::bad("name must be 1..=64 characters"));
            }
            if meta.description.chars().count() > 1_024 {
                return Err(ApiFailure::bad("description too long"));
            }
            if meta.tags.len() > MAX_TAGS || meta.tags.iter().any(|t| t.is_empty() || t.len() > 24)
            {
                return Err(ApiFailure::bad("too many or invalid tags"));
            }

            let level_bytes = match form.get("level") {
                Some(FormEntry::File(f)) => f.bytes().await?,
                _ => return Err(ApiFailure::bad("missing 'level' file field")),
            };
            if level_bytes.is_empty() || level_bytes.len() > MAX_UPLOAD_BYTES {
                return Err(ApiFailure::bad(format!(
                    "level size must be 1..={MAX_UPLOAD_BYTES} bytes"
                )));
            }

            // Verify the payload actually decodes and is structurally valid so
            // the bucket never fills with junk. `author` is server-set below.
            let level = decode_level(&level_bytes)?;
            validate_level(&level)?;

            let sha = hex::encode(sha2::Sha256::digest(&level_bytes));

            let db = ctx.env.d1(DB)?;

            // Cheap spam guard: reject exact re-uploads (the sha256 is already
            // in LevelMeta and validated to be an exact match on the client).
            let dup = db
                .prepare("SELECT id FROM levels WHERE sha256 = ? LIMIT 1")
                .bind(&[JsValue::from(sha.as_str())])?
                .first::<i64>(None)
                .await?;
            if dup.is_some() {
                return Err(ApiFailure::new(409, "identical level already uploaded"));
            }

            // Claim a quota slot *before* inserting. Admin uploads skip the
            // quota and are recorded as owner_id = ''.
            let slot: Option<i64> = match &caller {
                Some(c) => {
                    let now = Utc::now().timestamp();
                    ensure_owner_and_device(&db, c, now).await?;
                    match claim_upload_slot(&db, c, now).await? {
                        Some(id) => Some(id),
                        None => {
                            let quota = owner_quota(&db, &c.owner_id, now).await?;
                            let retry_after =
                                quota.oldest.map(|t| ((t + WEEK_SECS) - now).max(0));
                            return Err(ApiFailure {
                                status: 429,
                                msg: match retry_after {
                                    Some(s) => format!(
                                        "weekly upload limit reached; retry_after_secs={s}"
                                    ),
                                    None => "upload limit reached".to_string(),
                                },
                            });
                        }
                    }
                }
                None => None,
            };

            let owner_id = caller.map(|c| c.owner_id).unwrap_or_default();
            let tags_json = serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into());
            let row = db
                .prepare(
                    "INSERT INTO levels \
                     (author, name, description, tags, format_version, game_version, size_bytes, sha256, status, owner_id) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'published', ?) \
                     RETURNING id, author, name, description, tags, format_version, game_version, \
                               size_bytes, sha256, likes, plays, status, owner_id, created_at, updated_at",
                )
                .bind(&[
                    JsValue::from("alpha-tester"),
                    JsValue::from(meta.name.as_str()),
                    JsValue::from(meta.description.as_str()),
                    JsValue::from(tags_json.as_str()),
                    JsValue::from(meta.format_version as f64),
                    JsValue::from(meta.game_version.as_str()),
                    JsValue::from(level_bytes.len() as f64),
                    JsValue::from(sha.as_str()),
                    JsValue::from(owner_id.as_str()),
                ])?
                .first::<LevelRow>(None)
                .await;
            let row = match row {
                Ok(Some(row)) => row,
                Ok(None) => return Err(ApiFailure::new(500, "insert returned no row")),
                Err(e) => {
                    if let Some(slot_id) = slot {
                        let _ = release_upload_slot(&db, slot_id).await;
                    }
                    return Err(e.into());
                }
            };

            let key = format!("levels/{}.bin", row.id);
            let bucket = ctx.env.bucket(BUCKET)?;
            if let Err(e) = bucket.put(key, level_bytes).execute().await {
                if let Some(slot_id) = slot {
                    let _ = release_upload_slot(&db, slot_id).await;
                }
                return Err(e.into());
            }

            let meta_row = row.into_meta();
            json_ok(
                &ctx.env,
                &UploadResponse {
                    id: meta_row.id,
                    meta: meta_row,
                },
                201,
            )
        }
        .await,
    )
}

async fn report_level(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let ip = client_ip(&req);
            check_rate(&ctx.env, "report", &ip, GENERAL_PER_WINDOW).await?;
            let db = ctx.env.d1(DB)?;
            let id: u64 = ctx
                .param("id")
                .ok_or_else(|| ApiFailure::bad("bad level id"))?
                .parse()
                .map_err(|_| ApiFailure::bad("bad level id"))?;
            if row_by_id(&db, id).await?.is_none() {
                return Err(ApiFailure::not_found("level not found"));
            }
            db.prepare("UPDATE levels SET reports = reports + 1 WHERE id = ?")
                .bind(&[JsValue::from(id as f64)])?
                .run()
                .await?;
            empty_ok(&ctx.env, 204)
        }
        .await,
    )
}

async fn like_level(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let ip = client_ip(&req);
            check_rate(&ctx.env, "like", &ip, GENERAL_PER_WINDOW).await?;
            let db = ctx.env.d1(DB)?;
            let id: u64 = ctx
                .param("id")
                .ok_or_else(|| ApiFailure::bad("bad level id"))?
                .parse()
                .map_err(|_| ApiFailure::bad("bad level id"))?;
            if row_by_id(&db, id).await?.is_none() {
                return Err(ApiFailure::not_found("level not found"));
            }
            db.prepare("UPDATE levels SET likes = likes + 1 WHERE id = ?")
                .bind(&[JsValue::from(id as f64)])?
                .run()
                .await?;
            empty_ok(&ctx.env, 204)
        }
        .await,
    )
}

async fn delete_level(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    finish(
        &ctx.env,
        async {
            let db = ctx.env.d1(DB)?;
            let bucket = ctx.env.bucket(BUCKET)?;
            let id: u64 = ctx
                .param("id")
                .ok_or_else(|| ApiFailure::bad("bad level id"))?
                .parse()
                .map_err(|_| ApiFailure::bad("bad level id"))?;
            let Some(row) = row_by_id(&db, id).await? else {
                return Err(ApiFailure::not_found("level not found"));
            };

            let is_admin = is_authorized(&ctx.env, &req);
            let is_owner = extract_caller(&req)
                .ok()
                .is_some_and(|c| !row.owner_id.is_empty() && c.owner_id == row.owner_id);

            if !is_admin && !is_owner {
                return Err(ApiFailure::new(403, "not your level"));
            }

            let _ = bucket.delete(format!("levels/{}.bin", row.id)).await;
            db.prepare("UPDATE levels SET status = 'deleted' WHERE id = ?")
                .bind(&[JsValue::from(id as f64)])?
                .run()
                .await?;
            empty_ok(&ctx.env, 204)
        }
        .await,
    )
}
