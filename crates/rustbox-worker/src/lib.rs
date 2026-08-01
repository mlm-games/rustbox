//! rustbox online level-sharing backend.
//!
//! Cloudflare Worker (Rust wasm32) backed by D1 (metadata) + R2 (level
//! files). The API is intentionally small and public for reads, with
//! token-gated writes for the alpha phase:
//!
//! ```text
//! GET    /v1/levels                     list published levels
//! GET    /v1/levels/:id                 single level metadata
//! GET    /v1/levels/:id/data            raw level bytes (bincode+deflate)
//! POST   /v1/levels                     multipart upload (auth token required)
//! POST   /v1/levels/:id/report          report a level (per-IP rate limited)
//! POST   /v1/levels/:id/like            like a level (per-IP rate limited)
//! DELETE /v1/levels/:id                 delete (auth token required)
//! OPTIONS ...                           CORS preflight
//! ```
//!
//! All read responses carry CORS headers so the WASM build on GitHub Pages
//! can fetch them. (CORS is *not* auth,  desktop/Android clients ignore it, so
//! writes are protected by `UPLOAD_TOKEN` (a Worker secret) for now.)

use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use wasm_bindgen::JsValue;
use worker::d1::D1Database;
use worker::{
    event, Cors, Env, FormEntry, Headers, Method, Request, Response, Result, RouteContext, Router,
};

use rustbox_format::api::{ApiError, LevelListResponse, LevelMeta, UploadMetadata, UploadResponse};
use rustbox_format::file::{decode_level, validate_level};
use rustbox_format::{API_VERSION, MAX_TAGS, MAX_UPLOAD_BYTES};

const DB: &str = "DB";
const BUCKET: &str = "LEVELS";
const LIST_LIMIT_MAX: u64 = 100;
const RATE_WINDOW_SECS: i64 = 300; // 5 minutes
const UPLOADS_PER_WINDOW: u32 = 3;
const GENERAL_PER_WINDOW: u32 = 60;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> Result<Response> {
    Router::new()
        .get_async("/v1/levels", list_levels)
        .get_async("/v1/levels/:id", get_level)
        .get_async("/v1/levels/:id/data", get_level_data)
        .post_async("/v1/levels", upload_level)
        .post_async("/v1/levels/:id/report", report_level)
        .post_async("/v1/levels/:id/like", like_level)
        .delete_async("/v1/levels/:id", delete_level)
        .options_async("/v1/levels", preflight)
        .options_async("/v1/levels/:id", preflight)
        .options_async("/v1/levels/:id/data", preflight)
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
        .with_allowed_headers(["Content-Type", "X-Auth-Token"])
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

fn query_params(req: &Request) -> HashMap<String, String> {
    let Ok(url) = req.url() else {
        return HashMap::new();
    };
    url.query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect()
}

/// Sliding-window counter in D1. Returns Ok(()) if under the limit.
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
            if !is_authorized(&ctx.env, &req) {
                return Err(ApiFailure::new(401, "missing or invalid upload token"));
            }
            let ip = client_ip(&req);
            check_rate(&ctx.env, "upload", &ip, UPLOADS_PER_WINDOW).await?;

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
            let tags_json = serde_json::to_string(&meta.tags).unwrap_or_else(|_| "[]".into());
            let row = db
                .prepare(
                    "INSERT INTO levels \
                     (author, name, description, tags, format_version, game_version, size_bytes, sha256, status) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'published') \
                     RETURNING id, author, name, description, tags, format_version, game_version, \
                               size_bytes, sha256, likes, plays, status, created_at, updated_at",
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
                ])?
                .first::<LevelRow>(None)
                .await?;
            let Some(row) = row else {
                return Err(ApiFailure::new(500, "insert returned no row"));
            };

            let key = format!("levels/{}.bin", row.id);
            let bucket = ctx.env.bucket(BUCKET)?;
            bucket.put(key, level_bytes).execute().await?;

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
            if !is_authorized(&ctx.env, &req) {
                return Err(ApiFailure::new(401, "missing or invalid upload token"));
            }
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
