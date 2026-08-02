//! Shared, pure-data level format for the rustbox game.
//!
//! This crate intentionally has no bevy deps, so that both the game
//! (native / Android / WASM) and the Cloudflare Worker backend can agree on a
//! single serialized format: `LevelData` (and friends) plus the
//! bincode + DEFLATE wire encoding (`encode_level` / `decode_level`) and the
//! copyable share-code string form (`export_code` / `import_code`).
//!
//! Gameplay helpers that need Bevy math (colors, `IVec3`, gizmos, ...) live in
//! the game crate as extension traits, not here.

pub mod api;
pub mod block;
pub mod entity;
pub mod file;
pub mod level;
pub mod track;

pub use api::{
    API_VERSION, ApiError, LevelListResponse, LevelMeta, MAX_TAGS, MAX_UPLOAD_BYTES,
    UploadMetadata, UploadResponse,
};
pub use block::{ALL_BLOCK_KINDS, ALL_BLOCK_SHAPES, BlockKind, BlockShape};
pub use entity::{EntityData, EntityKind, LevelEntityId};
pub use file::{
    FORMAT_VERSION, LevelFile, decode_level, encode_level, export_code, import_code, validate_level,
};
pub use level::{BlockData, BoundaryConfig, BoundaryPreset, LevelData, LevelTag, Theme};
pub use track::{TrackData, TrackId, TrackMode};
