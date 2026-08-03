use std::collections::HashMap;
use std::io::Cursor;

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender, unbounded};

use rustbox_format::api::{ApiError, LevelListResponse, LevelMeta, UploadMetadata, UploadResponse};
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
    /// Fetch a single level's metadata by server id (MM2-style "Search with ID").
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
}

/// Optional upload token (never compiled into the binary). Only uploads and
/// deletes need it; browsing and downloading work anonymously.
pub struct OnlineConfig {
    pub base_url: String,
    pub token: String,
}

/// Everything the UI needs to make an online request. Held as one resource so
/// the (already large) `drain_ui_commands` system stays within Bevy's 16-param
/// limit: commands push into `pending`, which `flush_online_requests` drains.
#[derive(Resource)]
pub struct OnlineContext {
    pub config: OnlineConfig,
    pub tx: Sender<OnlineEvent>,
    pub pending: Vec<OnlineRequest>,
}

#[derive(Resource)]
pub struct OnlineEventRx(pub Receiver<OnlineEvent>);

/// Cache of downloaded levels, keyed by server id, so re-downloads are free.
#[derive(Resource, Default)]
pub struct OnlineCache(pub HashMap<u64, LevelData>);

/// Most recently fetched listing, so the UI can re-render without a round trip.
#[derive(Resource, Default)]
pub struct OnlineListing(pub Vec<LevelMeta>);

impl Default for OnlineConfig {
    fn default() -> Self {
        Self {
            base_url: api_base().to_string(),
            token: String::new(),
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

fn with_auth(cfg: &OnlineConfig, req: ehttp::Request) -> ehttp::Request {
    if cfg.token.trim().is_empty() {
        req
    } else {
        req.with_header("X-Auth-Token", cfg.token.trim())
    }
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

/// Forward a request to a fetch. Called from the UI command drainer (main
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
            let url = format!("{base}/v1/levels/{}/data", meta.id);
            ehttp::fetch(ehttp::Request::get(url), move |result| {
                let event = match result {
                    Ok(resp) if resp.ok && !resp.bytes.is_empty() => OnlineEvent::Downloaded {
                        result: decode_level(&resp.bytes).map_err(|e| e.to_string()),
                        meta,
                        play,
                    },
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
            send_empty(ehttp::Request::post(url, Vec::new()), ev_tx, move |r| {
                OnlineEvent::Liked { id, result: r }
            });
        }
        OnlineRequest::Report { id } => {
            let url = format!("{base}/v1/levels/{id}/report");
            send_empty(ehttp::Request::post(url, Vec::new()), ev_tx, move |r| {
                OnlineEvent::Reported { id, result: r }
            });
        }
        OnlineRequest::Delete { id } => {
            let url = format!("{base}/v1/levels/{id}");
            send_empty(
                with_auth(cfg, ehttp::Request::delete(&url)),
                ev_tx,
                move |r| OnlineEvent::Deleted { id, result: r },
            );
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

pub struct OnlinePlugin;

impl Plugin for OnlinePlugin {
    fn build(&self, app: &mut App) {
        let (ev_tx, ev_rx) = unbounded();
        let ctx = OnlineContext {
            config: OnlineConfig::default(),
            tx: ev_tx.clone(),
            pending: Vec::new(),
        };
        app.insert_resource(ctx)
            .insert_resource(OnlineEventRx(ev_rx))
            .insert_resource(OnlineCache::default())
            .insert_resource(OnlineListing::default())
            .add_systems(Update, flush_online_requests)
            .add_systems(Update, poll_online_events);
    }
}

/// Drain UI-queued online requests into fetches (non-blocking; responses arrive
/// later on the event channel) and sync the upload token from the UI. Download
/// requests for already-cached levels are answered locally without a round trip.
pub fn flush_online_requests(
    mut ctx: ResMut<OnlineContext>,
    cache: Res<OnlineCache>,
    mut ui: ResMut<super::ui_bridge::MakerUi>,
    mut level: ResMut<super::level::LevelDocument>,
    mut history: ResMut<super::commands::CommandHistory>,
    mut mode: ResMut<super::mode::MakerMode>,
    mut source: ResMut<super::campaign::LevelSource>,
    mut sel_ent: ResMut<super::mode::SelectedEntity>,
) {
    for req in std::mem::take(&mut ctx.pending) {
        if let OnlineRequest::Download { meta, play } = &req {
            if let Some(data) = cache.0.get(&meta.id).cloned() {
                if !play {
                    let preview = super::thumbnail::render_preview(
                        &data,
                        super::catalog::PREVIEW_COLS,
                        super::catalog::PREVIEW_ROWS,
                    );
                    ui.online_preview_pending.retain(|id| *id != meta.id);
                    ui.online_previews.insert(meta.id, preview);
                    touch_online_preview_lru(&mut ui, meta.id);
                }
                apply_download(
                    &mut level,
                    &mut history,
                    &mut mode,
                    &mut source,
                    &mut sel_ent,
                    meta,
                    data,
                    *play,
                );
                continue;
            }
        }
        dispatch(&ctx.config, &ctx.tx, req);
    }
    if ctx.config.token != ui.online_token {
        ctx.config.token = ui.online_token.trim().to_string();
    }
}

/// Bound the generated-preview cache: mark `id` most-recently-used and evict
/// the oldest previews once we pass `MAX_PREVIEWS_CACHED`.
pub fn touch_online_preview_lru(ui: &mut super::ui_bridge::MakerUi, id: u64) {
    const MAX_PREVIEWS_CACHED: usize = 96;

    ui.online_preview_lru.retain(|x| *x != id);
    ui.online_preview_lru.push(id);

    while ui.online_preview_lru.len() > MAX_PREVIEWS_CACHED {
        let evict = ui.online_preview_lru.remove(0);
        ui.online_previews.remove(&evict);
    }
    ui.online_preview_pending.retain(|x| *x != id);
}

fn apply_download(
    level: &mut super::level::LevelDocument,
    history: &mut super::commands::CommandHistory,
    mode: &mut super::mode::MakerMode,
    source: &mut super::campaign::LevelSource,
    sel_ent: &mut super::mode::SelectedEntity,
    meta: &LevelMeta,
    data: LevelData,
    play: bool,
) {
    if play {
        super::storage::apply_level_data(level, history, data);
        level.data.name = meta.name.clone();
        level.data.author = meta.author.clone();
        *sel_ent = super::mode::SelectedEntity(None);
        *source = super::campaign::LevelSource::Imported;
        *mode = super::mode::MakerMode::Play;
    }
}

/// Drain completed fetch callbacks on the main thread and apply side effects:
/// cache downloaded levels, refresh the listing, surface errors as status text.
pub fn poll_online_events(
    events: Res<OnlineEventRx>,
    mut listing: ResMut<OnlineListing>,
    mut cache: ResMut<OnlineCache>,
    mut ui: ResMut<super::ui_bridge::MakerUi>,
    mut level: ResMut<super::level::LevelDocument>,
    mut history: ResMut<super::commands::CommandHistory>,
    mut mode: ResMut<super::mode::MakerMode>,
    mut source: ResMut<super::campaign::LevelSource>,
    mut sel_ent: ResMut<super::mode::SelectedEntity>,
) {
    while let Ok(event) = events.0.try_recv() {
        match event {
            OnlineEvent::Listed(result) => match result {
                Ok(resp) => {
                    ui.online_loading = false;
                    ui.online_levels = resp.levels;
                    listing.0 = ui.online_levels.clone();
                    ui.online_confirm_delete = None;
                    super::ui_bridge::reconcile_online_nav(&mut ui);
                    ui.set_status(format!("{} levels online", listing.0.len()));
                }
                Err(e) => {
                    ui.online_loading = false;
                    ui.set_status(format!("Browse failed: {e}"));
                }
            },
            OnlineEvent::FetchedById { id, result } => match result {
                Ok(meta) => {
                    ui.online_loading = false;
                    ui.online_levels = vec![meta];
                    ui.online_selected = Some(id);
                    ui.online_confirm_delete = None;
                    super::ui_bridge::reconcile_online_nav(&mut ui);
                    ui.set_status(format!("Found #{id}"));
                }
                Err(e) => {
                    ui.online_loading = false;
                    ui.set_status(format!("ID search ({id}): {e}"));
                }
            },
            OnlineEvent::Uploaded(result) => match result {
                Ok(resp) => ui.set_status(format!("Published as #{}", resp.id)),
                Err(e) => ui.set_status(format!("Upload failed: {e}")),
            },
            OnlineEvent::Downloaded { meta, result, play } => match result {
                Ok(data) => {
                    let preview = super::thumbnail::render_preview(
                        &data,
                        super::catalog::PREVIEW_COLS,
                        super::catalog::PREVIEW_ROWS,
                    );

                    ui.online_preview_pending.retain(|id| *id != meta.id);
                    ui.online_previews.insert(meta.id, preview);
                    cache.0.insert(meta.id, data.clone());
                    touch_online_preview_lru(&mut ui, meta.id);
                    apply_download(
                        &mut level,
                        &mut history,
                        &mut mode,
                        &mut source,
                        &mut sel_ent,
                        &meta,
                        data,
                        play,
                    );
                    if play {
                        ui.set_status(format!("Downloaded & playing: {}", level.data.name));
                    } else {
                        ui.set_status(format!("Downloaded: {}", meta.name));
                    }
                }
                Err(e) => {
                    ui.online_preview_pending.retain(|id| *id != meta.id);
                    ui.set_status(format!("Download failed: {e}"));
                }
            },
            OnlineEvent::Liked { id, result } => match result {
                Ok(()) => ui.set_status(format!("Liked #{id}")),
                Err(e) => ui.set_status(format!("Like failed: {e}")),
            },
            OnlineEvent::Reported { id, result } => match result {
                Ok(()) => ui.set_status(format!("Reported #{id} - thanks!")),
                Err(e) => ui.set_status(format!("Report failed: {e}")),
            },
            OnlineEvent::Deleted { id, result } => match result {
                Ok(()) => {
                    cache.0.remove(&id);
                    ui.online_previews.remove(&id);
                    ui.online_preview_pending.retain(|x| *x != id);
                    ui.online_preview_lru.retain(|x| *x != id);
                    ui.online_levels.retain(|m| m.id != id);
                    listing.0.retain(|m| m.id != id);

                    if ui.online_selected == Some(id) {
                        ui.online_selected = None;
                    }

                    ui.online_confirm_delete = None;
                    super::ui_bridge::reconcile_online_nav(&mut ui);
                    ui.set_status(format!("Deleted #{id}"));
                }
                Err(e) => ui.set_status(format!("Delete failed: {e}")),
            },
        }
    }
}
