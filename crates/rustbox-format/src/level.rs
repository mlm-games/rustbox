use serde::{Deserialize, Serialize};

use crate::block::BlockKind;
use crate::entity::EntityData;
use crate::track::TrackData;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockData {
    pub position: [i32; 3],
    pub kind: BlockKind,
}

fn level_format_entities() -> u32 {
    1
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
}
