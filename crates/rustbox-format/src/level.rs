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

/// Auto-generated play-area boundaries (floor / walls / ceiling), mirroring
/// MB64's `mb64_lopt_boundary*` options and `mb64_boundary_table` presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryConfig {
    /// Solid catchable floor beneath the play area (keeps you out of the void).
    #[serde(default)]
    pub inner_floor: bool,
    /// A floor layer ringing the level on the far side of the rim.
    #[serde(default)]
    pub outer_floor: bool,
    /// Solid rim walls around the level sides (prevent walking off the edge).
    #[serde(default)]
    pub inner_walls: bool,
    /// Low perimeter lip (MB64's "Plateau"), a shallow step at the base.
    #[serde(default)]
    pub outer_walls: bool,
    /// Solid ceiling above the level (prevents jumping out).
    #[serde(default)]
    pub ceiling: bool,
    /// Wall/room height in cells. 0 = auto (derived from level size/content).
    #[serde(default)]
    pub height: i32,
}

/// Named boundary presets, matching MB64's boundary table. "Chasm" is renamed
/// "Warp": an intentional gap/opening you cross (in MB64 a pit with outer floor).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BoundaryPreset {
    #[default]
    Void,
    Plain,
    Valley,
    Warp,
    Plateau,
    Interior,
}

impl BoundaryPreset {
    pub const ALL: [BoundaryPreset; 6] = [
        BoundaryPreset::Void,
        BoundaryPreset::Plain,
        BoundaryPreset::Valley,
        BoundaryPreset::Warp,
        BoundaryPreset::Plateau,
        BoundaryPreset::Interior,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BoundaryPreset::Void => "Void",
            BoundaryPreset::Plain => "Plain",
            BoundaryPreset::Valley => "Valley",
            BoundaryPreset::Warp => "Warp",
            BoundaryPreset::Plateau => "Plateau",
            BoundaryPreset::Interior => "Interior",
        }
    }

    pub fn config(self) -> BoundaryConfig {
        match self {
            BoundaryPreset::Void => BoundaryConfig {
                inner_floor: false,
                outer_floor: false,
                inner_walls: false,
                outer_walls: false,
                ceiling: false,
                height: 0,
            },
            BoundaryPreset::Plain => BoundaryConfig {
                inner_floor: true,
                outer_floor: false,
                inner_walls: false,
                outer_walls: false,
                ceiling: false,
                height: 0,
            },
            BoundaryPreset::Valley => BoundaryConfig {
                inner_floor: true,
                outer_floor: false,
                inner_walls: true,
                outer_walls: false,
                ceiling: false,
                height: 0,
            },
            // MB64 "Chasm": outer floor + inner rim, an open pit you fall into.
            BoundaryPreset::Warp => BoundaryConfig {
                inner_floor: false,
                outer_floor: true,
                inner_walls: true,
                outer_walls: false,
                ceiling: false,
                height: 0,
            },
            BoundaryPreset::Plateau => BoundaryConfig {
                inner_floor: true,
                outer_floor: false,
                inner_walls: false,
                outer_walls: true,
                ceiling: false,
                height: 0,
            },
            BoundaryPreset::Interior => BoundaryConfig {
                inner_floor: true,
                outer_floor: false,
                inner_walls: true,
                outer_walls: false,
                ceiling: true,
                height: 0,
            },
        }
    }
}

impl Default for BoundaryConfig {
    fn default() -> Self {
        Self {
            inner_floor: false,
            outer_floor: false,
            inner_walls: true,
            outer_walls: false,
            ceiling: false,
            height: 0,
        }
    }
}

impl BoundaryConfig {
    /// The preset whose flags match this config, ignoring the independent
    /// `height`. `None` when the flags are a custom combination.
    pub fn boundary_preset(self) -> Option<BoundaryPreset> {
        BoundaryPreset::ALL
            .into_iter()
            .find(|&p| p.config().matches_flags(&self))
    }

    fn matches_flags(&self, other: &BoundaryConfig) -> bool {
        self.inner_floor == other.inner_floor
            && self.outer_floor == other.outer_floor
            && self.inner_walls == other.inner_walls
            && self.outer_walls == other.outer_walls
            && self.ceiling == other.ceiling
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
