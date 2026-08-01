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
