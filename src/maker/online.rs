use std::collections::HashMap;
use std::io::Cursor;

use crossbeam_channel::{Receiver, Sender, unbounded};

pub use rustbox_format::api::LevelMeta;
use rustbox_format::api::{
    ApiError, LevelListResponse, MeResponse, UploadMetadata, UploadResponse,
};
use rustbox_format::file::decode_level;
use rustbox_format::level::LevelData;

/// Backend root (Override at build time with `RUSTBOX_API_BASE` if you run
/// `wrangler dev` locally)
pub const DEFAULT_API_BASE: &str = "https://rustbox-api.mlm-games.workers.dev";

pub fn api_base() -> &'static str {
    match option_env!("RUSTBOX_API_BASE") {
        Some(v) => v,
        None => DEFAULT_API_BASE,
    }
}

/// A request the UI wants to make. `dispatch` turns it into a (non-blocking)
/// `ehttp` fetch and forwards the outcome onto the event channel.
#[derive(Debug)]
pub enum OnlineRequest {
    List {
        query: String,
        limit: u64,
        offset: u64,
    },
    /// Fetch a single level's metadata by server id (maker2-style "Search with ID").
    FetchById(u64),
    Upload {
        meta: UploadMetadata,
        data: LevelData,
    },
    Download {
        meta: LevelMeta,
        play: bool,
    },
    Like {
        id: u64,
    },
    Report {
        id: u64,
    },
    Delete {
        id: u64,
    },
    /// Fetch `/v1/me` (identity + weekly upload quota) to display status.
    Me,
    /// Fetch `/v1/me/levels` - the creator's own published levels.
    MyLevels,
}

#[derive(Debug)]
pub enum OnlineEvent {
    Listed(Result<LevelListResponse, String>),
    /// A single `GET /v1/levels/:id` lookup for the "Search with ID" field.
    FetchedById {
        id: u64,
        result: Result<LevelMeta, String>,
    },
    Uploaded(Result<UploadResponse, String>),
    Downloaded {
        meta: LevelMeta,
        result: Result<LevelData, String>,
        play: bool,
    },
    Liked {
        id: u64,
        result: Result<(), String>,
    },
    Reported {
        id: u64,
        result: Result<(), String>,
    },
    Deleted {
        id: u64,
        result: Result<(), String>,
    },
    Me(Result<MeResponse, String>),
    MyLevels(Result<LevelListResponse, String>),
}

/// Optional upload token (never compiled into the binary). Only uploads and
/// deletes need it; browsing and downloading work anonymously.
pub struct OnlineConfig {
    pub base_url: String,
    pub token: String,
    /// Anonymous creator recovery key (`Authorization: Bearer`). Sent on
    /// upload/delete/me/my-levels; ownership and quota ride on it.
    pub recovery_key: String,
    /// Local-only device id (`X-Rustbox-Device`), a secondary abuse signal.
    pub device_id: String,
}

impl Default for OnlineConfig {
    fn default() -> Self {
        Self {
            base_url: api_base().to_string(),
            token: String::new(),
            recovery_key: String::new(),
            device_id: String::new(),
        }
    }
}

/// Runtime half of the online feature: request config, the event channel and
/// the downloaded-level cache. Pending requests live in `MenuState`; each frame
/// `pump_online` dispatches them and drains completed fetches back.
pub struct OnlineRuntime {
    pub config: OnlineConfig,
    pub tx: Sender<OnlineEvent>,
    pub rx: Receiver<OnlineEvent>,
    /// Cache of downloaded levels, keyed by server id, so re-downloads are free.
    pub cache: HashMap<u64, LevelData>,
}

impl Default for OnlineRuntime {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        Self {
            config: OnlineConfig::default(),
            tx,
            rx,
            cache: HashMap::new(),
        }
    }
}

fn base_url(cfg: &OnlineConfig) -> String {
    if cfg.base_url.trim().is_empty() {
        DEFAULT_API_BASE.to_string()
    } else {
        cfg.base_url.trim().to_string()
    }
}

/// Attach whatever write-credentials the client has: the admin token (if set)
/// and the anonymous creator identity (recovery key + device id). Identity
/// headers are only meaningful once `pump_online` bootstrapped them.
fn with_auth(cfg: &OnlineConfig, req: ehttp::Request) -> ehttp::Request {
    let mut req = req;
    if !cfg.token.trim().is_empty() {
        req = req.with_header("X-Auth-Token", cfg.token.trim());
    }
    if !cfg.recovery_key.is_empty() {
        req = req.with_header("Authorization", &format!("Bearer {}", cfg.recovery_key));
    }
    if !cfg.device_id.is_empty() {
        req = req.with_header("X-Rustbox-Device", &cfg.device_id);
    }
    req
}

/// Kick off a fetch whose success body parses as `T` and forward the outcome
/// onto the event channel. `ehttp::fetch` is non-blocking on both native and
/// wasm and invokes its callback from a background thread, so we only touch the
/// thread-safe sender.
fn send<T: serde::de::DeserializeOwned + 'static>(
    request: ehttp::Request,
    ev_tx: Sender<OnlineEvent>,
    build: impl FnOnce(Result<T, String>) -> OnlineEvent + Send + 'static,
) {
    ehttp::fetch(request, move |result| {
        let outcome = match result {
            Ok(resp) if resp.ok => parse_json::<T>(&resp),
            Ok(resp) => Err(parse_error(resp)),
            Err(e) => Err(nonempty(e)),
        };
        let _ = ev_tx.send(build(outcome));
    });
}

/// Like `send`, for endpoints that succeed with an empty body (like / report / delete).
fn send_empty(
    request: ehttp::Request,
    ev_tx: Sender<OnlineEvent>,
    build: impl FnOnce(Result<(), String>) -> OnlineEvent + Send + 'static,
) {
    ehttp::fetch(request, move |result| {
        let outcome = match result {
            Ok(resp) if resp.ok => Ok(()),
            Ok(resp) => Err(parse_error(resp)),
            Err(e) => Err(nonempty(e)),
        };
        let _ = ev_tx.send(build(outcome));
    });
}

/// Forward a request to a fetch. Called from the per-frame pump (main
/// thread); the fetch itself returns immediately and the response arrives
/// later on the event channel.
pub fn dispatch(cfg: &OnlineConfig, ev_tx: &Sender<OnlineEvent>, req: OnlineRequest) {
    let base = base_url(cfg);
    let ev_tx = ev_tx.clone();
    match req {
        OnlineRequest::List {
            query,
            limit,
            offset,
        } => {
            let mut url = format!("{base}/v1/levels?limit={limit}&offset={offset}");
            if !query.is_empty() {
                url.push_str("&q=");
                url.push_str(&urlencode(&query));
            }
            send(ehttp::Request::get(url), ev_tx, |r| OnlineEvent::Listed(r));
        }
        OnlineRequest::FetchById(id) => {
            let url = format!("{base}/v1/levels/{id}");
            send(ehttp::Request::get(url), ev_tx, move |r| {
                OnlineEvent::FetchedById { id, result: r }
            });
        }
        OnlineRequest::Upload { meta, data } => {
            let url = format!("{base}/v1/levels");
            let bytes = match rustbox_format::file::encode_level(&data) {
                Ok(b) => b,
                Err(e) => {
                    let _ = ev_tx.send(OnlineEvent::Uploaded(Err(e.to_string())));
                    return;
                }
            };
            let meta_json = serde_json::to_string(&meta).unwrap_or_else(|_| "{}".into());
            let builder = ehttp::multipart::MultipartBuilder::new()
                .add_text("metadata", &meta_json)
                .add_stream(&mut Cursor::new(bytes), "level", Some("level.bin"), None);
            let builder = match builder {
                Ok(b) => b,
                Err(e) => {
                    let _ = ev_tx.send(OnlineEvent::Uploaded(Err(e.to_string())));
                    return;
                }
            };
            send(
                with_auth(cfg, ehttp::Request::post_multipart(url, builder)),
                ev_tx,
                |r| OnlineEvent::Uploaded(r),
            );
        }
        OnlineRequest::Download { meta, play } => {
            // `count=1` only for real plays: previews share the endpoint and
            // must not inflate the server `plays` counter.
            let url = if play {
                format!("{base}/v1/levels/{}/data?count=1", meta.id)
            } else {
                format!("{base}/v1/levels/{}/data", meta.id)
            };
            ehttp::fetch(ehttp::Request::get(url), move |result| {
                let event = match result {
                    Ok(resp) if resp.ok && !resp.bytes.is_empty() => {
                        let decoded = decode_level(&resp.bytes).map_err(|e| e.to_string());
                        let checked = decoded.and_then(|data| {
                            rustbox_format::file::validate_level(&data)
                                .map(|()| data)
                                .map_err(|e| format!("invalid level: {e}"))
                        });
                        OnlineEvent::Downloaded {
                            result: checked,
                            meta,
                            play,
                        }
                    }
                    Ok(resp) => OnlineEvent::Downloaded {
                        result: Err(parse_error(resp)),
                        meta,
                        play,
                    },
                    Err(e) => OnlineEvent::Downloaded {
                        result: Err(nonempty(e)),
                        meta,
                        play,
                    },
                };
                let _ = ev_tx.send(event);
            });
        }
        OnlineRequest::Like { id } => {
            let url = format!("{base}/v1/levels/{id}/like");
            send_empty(
                with_auth(cfg, ehttp::Request::post(url, Vec::new())),
                ev_tx,
                move |r| OnlineEvent::Liked { id, result: r },
            );
        }
        OnlineRequest::Report { id } => {
            let url = format!("{base}/v1/levels/{id}/report");
            send_empty(
                with_auth(cfg, ehttp::Request::post(url, Vec::new())),
                ev_tx,
                move |r| OnlineEvent::Reported { id, result: r },
            );
        }
        OnlineRequest::Delete { id } => {
            let url = format!("{base}/v1/levels/{id}");
            send_empty(
                with_auth(cfg, ehttp::Request::delete(&url)),
                ev_tx,
                move |r| OnlineEvent::Deleted { id, result: r },
            );
        }
        OnlineRequest::Me => {
            let url = format!("{base}/v1/me");
            send(with_auth(cfg, ehttp::Request::get(url)), ev_tx, |r| {
                OnlineEvent::Me(r)
            });
        }
        OnlineRequest::MyLevels => {
            let url = format!("{base}/v1/me/levels");
            send(with_auth(cfg, ehttp::Request::get(url)), ev_tx, |r| {
                OnlineEvent::MyLevels(r)
            });
        }
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn parse_json<T: serde::de::DeserializeOwned>(resp: &ehttp::Response) -> Result<T, String> {
    serde_json::from_slice(&resp.bytes).map_err(|e| e.to_string())
}

fn parse_error(resp: ehttp::Response) -> String {
    if let Ok(err) = serde_json::from_slice::<ApiError>(&resp.bytes)
        && !err.error.is_empty()
    {
        err.error
    } else {
        format!("HTTP {} {}", resp.status, resp.status_text)
    }
}

fn nonempty(mut s: String) -> String {
    if s.trim().is_empty() {
        s = "HTTP error".to_string();
    }
    s
}

/// Order online levels the way the UI / grid navigation expects.
/// - `shelf == 1`: Popular (by likes)
/// - `shelf == 2`: Hot (recency-weighted engagement)
/// - `shelf == 3`: Mine (the caller's own levels, newest first)
/// - otherwise client-side secondary sort from `mode` (0 new, 1 name, 2 likes, 3 plays).
pub fn sort_online(levels: &[LevelMeta], mode: u8, shelf: u8) -> Vec<LevelMeta> {
    let mut v = levels.to_vec();
    if shelf == 1 {
        v.sort_by(|a, b| b.likes.cmp(&a.likes));
        return v;
    }
    if shelf == 2 {
        v.sort_by(|a, b| hot_of(b).total_cmp(&hot_of(a)));
        return v;
    }
    if shelf == 3 {
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        return v;
    }
    match mode % 4 {
        0 => v.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        1 => v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        2 => v.sort_by(|a, b| b.likes.cmp(&a.likes)),
        _ => v.sort_by(|a, b| b.plays.cmp(&a.plays)),
    }
    v
}

/// Recency-weighted "Hot" score: engagement normalized by how long the level
/// has been published, so genuinely new/rising levels float to the top.
pub fn hot_of(m: &LevelMeta) -> f64 {
    let engagement = m.likes as f64 * 4.0 + m.plays as f64;
    let age_hours = age_hours_from_created_at(&m.created_at).unwrap_or(72.0);
    engagement / (age_hours + 2.0).powf(1.4)
}

fn age_hours_from_created_at(created_at: &str) -> Option<f64> {
    const SECS_PER_DAY: i64 = 86_400;

    let digits = |field: &str, start: usize, len: usize| -> Option<i64> {
        field.get(start..start + len)?.parse::<i64>().ok()
    };

    // Parse the date prefix "YYYY-MM-DD" (ISO / sortable localtime formats).
    let mut year = digits(created_at, 0, 4)?;
    let month = digits(created_at, 5, 2)?;
    let day = digits(created_at, 8, 2)?;
    let (mut hour, mut minute, mut second) = (0i64, 0i64, 0i64);
    if let Some(rest) = created_at.get(11..) {
        hour = digits(&rest, 0, 2).unwrap_or(0);
        minute = digits(&rest, 3, 2).unwrap_or(0);
        second = digits(&rest, 6, 2).unwrap_or(0);
    }

    if month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }

    // Days-from-civil (Howard Hinnant) for a proleptic Gregorian date.
    year -= if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;

    let days_since_epoch = era * 146_097 + doe - 719_468;
    let created = days_since_epoch * SECS_PER_DAY + hour * 3600 + minute * 60 + second;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(created);

    Some(((now - created).max(0) as f64) / 3600.0)
}
