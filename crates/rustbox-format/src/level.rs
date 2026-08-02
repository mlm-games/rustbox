use serde::{Deserialize, Serialize};

use crate::block::{BlockKind, BlockShape};
use crate::entity::EntityData;
use crate::track::TrackData;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockData {
    pub position: [i32; 3],
    pub kind: BlockKind,
    #[serde(default)]
    pub shape: BlockShape,
    /// Yaw rotation in 90 degree steps (0..=3), applied clockwise around Y.
    #[serde(default)]
    pub rot: u8,
    /// Whether the cell this block sits in is also filled with water (the
    /// block is "waterlogged"). Rendered tinted, and the cell is swimmable.
    #[serde(default)]
    pub waterlogged: bool,
}

impl BlockData {
    pub fn new(position: [i32; 3], kind: BlockKind) -> Self {
        Self {
            position,
            kind,
            shape: BlockShape::Full,
            rot: 0,
            waterlogged: false,
        }
    }
}

fn level_format_entities() -> u32 {
    1
}

/// Environment theme for a level (sky, fog, ambient light, water tint).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Theme {
    #[default]
    Grass,
    Desert,
    Snow,
    Cave,
    Sky,
}

impl Theme {
    pub const ALL: [Theme; 5] = [
        Theme::Grass,
        Theme::Desert,
        Theme::Snow,
        Theme::Cave,
        Theme::Sky,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Grass => "Grassland",
            Theme::Desert => "Desert",
            Theme::Snow => "Snow",
            Theme::Cave => "Cave",
            Theme::Sky => "Sky",
        }
    }
}

/// Auto-generated play-area boundaries (walls / floor / ceiling), mirroring
/// MB64's `mb64_lopt_boundary*` options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryConfig {
    /// Solid walls around the level sides (prevent walking off the edge).
    pub walls: bool,
    /// Solid ceiling above the level (prevents jumping out).
    pub ceiling: bool,
    /// Wall height in cells. 0 = auto (derived from level size/content).
    #[serde(default)]
    pub height: i32,
}

impl Default for BoundaryConfig {
    fn default() -> Self {
        Self {
            walls: true,
            ceiling: false,
            height: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LevelData {
    pub name: String,
    pub spawn: [i32; 3],
    pub blocks: Vec<BlockData>,
    #[serde(default)]
    pub entities: Vec<EntityData>,
    #[serde(default)]
    pub tracks: Vec<TrackData>,
    #[serde(default = "level_format_entities")]
    pub entities_version: u32,
    #[serde(default)]
    pub author_time: Option<f32>,
    #[serde(default)]
    pub author_deaths: u32,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<LevelTag>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created_at: u64,
    /// Explicit play-area half-extents `[rx, ry, rz]`. `None` = auto-derived
    /// from level content.
    #[serde(default)]
    pub size: Option<[i32; 3]>,
    /// World-space Y of the water surface. `None` = no water.
    #[serde(default)]
    pub water_level: Option<i32>,
    /// Environment theme.
    #[serde(default)]
    pub theme: Theme,
    /// Boundary wall/ceiling config.
    #[serde(default)]
    pub boundary: BoundaryConfig,
    /// Number of secret stars hidden in the level.
    #[serde(default)]
    pub secret_stars: u8,
    /// Whether a star is earned for collecting the coin star.
    #[serde(default)]
    pub coin_star: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LevelTag {
    Short,
    Puzzle,
    Precision,
    Chill,
    Music,
    Auto,
}

impl LevelTag {
    pub const ALL: [LevelTag; 6] = [
        LevelTag::Short,
        LevelTag::Puzzle,
        LevelTag::Precision,
        LevelTag::Chill,
        LevelTag::Music,
        LevelTag::Auto,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LevelTag::Short => "Short",
            LevelTag::Puzzle => "Puzzle",
            LevelTag::Precision => "Precision",
            LevelTag::Chill => "Chill",
            LevelTag::Music => "Music",
            LevelTag::Auto => "Auto",
        }
    }
}
