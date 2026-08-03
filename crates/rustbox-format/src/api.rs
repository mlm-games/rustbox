//! Shared types for the online level-sharing API.
//!
//! Used by both the game client (uploads / browsing / downloads) and the
//! Cloudflare Worker backend so they cannot silently disagree about the wire
//! format. Pure data only.

use serde::{Deserialize, Serialize};

/// Wire version the API accepts for uploads. Must match `file::FORMAT_VERSION`.
pub const API_VERSION: u32 = 2;

/// Upper bound on a compressed level payload in a single upload (bytes).
pub const MAX_UPLOAD_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of tags on an upload.
pub const MAX_TAGS: usize = 4;

/// A single published level, as listed by `GET /v1/levels`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LevelMeta {
    pub id: u64,
    pub author: String,
    pub name: String,
    pub description: String,
    /// Tag names (e.g. `"Short"`, `"Precision"`). Opaque strings so the backend
    /// stays compatible with future tag sets.
    pub tags: Vec<String>,
    pub format_version: u32,
    pub game_version: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub likes: u64,
    pub plays: u64,
    /// UTC ISO-8601 timestamps (lexicographically sortable).
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LevelListResponse {
    pub levels: Vec<LevelMeta>,
    pub total: u64,
}

/// Multipart `metadata` field for `POST /v1/levels`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UploadMetadata {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub format_version: u32,
    pub game_version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UploadResponse {
    pub id: u64,
    pub meta: LevelMeta,
}

/// `GET /v1/me`, creator identity + weekly upload quota. The identity is
/// derived from the recovery key the client sends, so there is no registration
/// step; the first call silently creates the owner.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MeResponse {
    /// First 10 hex chars of the owner id (sha256 of the recovery key)
    pub owner_id_short: String,
    pub uploads_used_this_week: i64,
    pub uploads_remaining_this_week: i64,
    /// Unix seconds when the quota next frees a slot (or `None`).
    pub reset_at_unix: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiError {
    pub error: String,
}
