use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BlockKind {
    #[default]
    Grass,
    Stone,
    Hazard,
    Goal,
    Spawn,
    /// A cell-filling volume of water (swimmable, translucent, not solid).
    Water,
    Ice,
    Spikes,
    Conveyor,
    Bounce,
    Climb,
}

impl BlockKind {
    pub fn is_solid(self) -> bool {
        !matches!(self, BlockKind::Spawn | BlockKind::Water)
    }

    pub fn name(self) -> &'static str {
        match self {
            BlockKind::Grass => "Grass",
            BlockKind::Stone => "Stone",
            BlockKind::Hazard => "Hazard",
            BlockKind::Goal => "Goal",
            BlockKind::Spawn => "Spawn",
            BlockKind::Water => "Water",
            BlockKind::Ice => "Ice",
            BlockKind::Spikes => "Spikes",
            BlockKind::Conveyor => "Conveyor",
            BlockKind::Bounce => "Bounce",
            BlockKind::Climb => "Climb",
        }
    }
}

pub const ALL_BLOCK_KINDS: &[BlockKind] = &[
    BlockKind::Grass,
    BlockKind::Stone,
    BlockKind::Hazard,
    BlockKind::Goal,
    BlockKind::Spawn,
    BlockKind::Water,
    BlockKind::Ice,
    BlockKind::Spikes,
    BlockKind::Conveyor,
    BlockKind::Bounce,
    BlockKind::Climb,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BlockShape {
    /// Full 1x1x1 cube.
    #[default]
    Full,
    /// Bottom half slab: a 1x0.5x1 block sitting on the cell floor.
    Half,
    /// Top half slab: a 1x0.5x1 block hanging from the cell ceiling.
    TopHalf,
    /// 45 degree ramp rising to full height toward local +X.
    Slope,
    /// 45 degree ramp rising to full height toward local -X.
    DSlope,
    /// Quarter corner ramp rising toward the local +X,+Z corner.
    Corner,
    /// Quarter corner ramp rising toward the local -X,-Z corner (a peak).
    OuterCorner,
    /// Diagonal vertical slope: a full-height quarter along the -X/-Z corner,
    /// cut away along the diagonal plane (like MB64's vertical slope).
    VerticalSlope,
    /// Vertical half slab: a 1x1x0.5 slab against the local -Z face.
    VerticalSlab,
}

impl BlockShape {
    pub fn name(self) -> &'static str {
        match self {
            BlockShape::Full => "Block",
            BlockShape::Half => "Half",
            BlockShape::TopHalf => "Top Half",
            BlockShape::Slope => "Slope",
            BlockShape::DSlope => "Slope \u{2198}",
            BlockShape::Corner => "Corner",
            BlockShape::OuterCorner => "Outer Corner",
            BlockShape::VerticalSlope => "V Slope",
            BlockShape::VerticalSlab => "V Slab",
        }
    }
}

pub const ALL_BLOCK_SHAPES: &[BlockShape] = &[
    BlockShape::Full,
    BlockShape::Half,
    BlockShape::TopHalf,
    BlockShape::Slope,
    BlockShape::DSlope,
    BlockShape::Corner,
    BlockShape::OuterCorner,
    BlockShape::VerticalSlope,
    BlockShape::VerticalSlab,
];
