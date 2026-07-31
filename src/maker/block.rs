use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BlockKind {
    #[default]
    Grass,
    Stone,
    Hazard,
    Goal,
    Spawn,
}

impl BlockKind {
    pub fn color(self) -> Color {
        match self {
            BlockKind::Grass => Color::srgb(0.35, 0.72, 0.35),
            BlockKind::Stone => Color::srgb(0.55, 0.55, 0.60),
            BlockKind::Hazard => Color::srgb(0.85, 0.20, 0.20),
            BlockKind::Goal => Color::srgb(0.95, 0.82, 0.25),
            BlockKind::Spawn => Color::srgb(0.25, 0.55, 0.95),
        }
    }

    pub fn is_solid(self) -> bool {
        !matches!(self, BlockKind::Spawn)
    }

    pub fn name(self) -> &'static str {
        match self {
            BlockKind::Grass => "Grass",
            BlockKind::Stone => "Stone",
            BlockKind::Hazard => "Hazard",
            BlockKind::Goal => "Goal",
            BlockKind::Spawn => "Spawn",
        }
    }
}

pub const ALL_BLOCK_KINDS: &[BlockKind] = &[
    BlockKind::Grass,
    BlockKind::Stone,
    BlockKind::Hazard,
    BlockKind::Goal,
    BlockKind::Spawn,
];
