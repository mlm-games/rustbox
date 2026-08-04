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
    /// Conveyor rendered as a thin top slab (crawl space underneath).
    ThinConveyor,
    /// On/Off conveyor that runs while the switch state is ON.
    OnOffConveyorA,
    /// On/Off conveyor that runs while the switch state is OFF.
    OnOffConveyorB,
    /// Thin top slab you can hang under and traverse (no conveyor force).
    HangRail,
    /// One-way platform: solid only when approached from above (falling /
    /// standing), pass-through from below and from the sides.
    OneWay,
    /// Timed pulse block: solid while the global pulse is ON, empty while OFF.
    TimedPulse,
}

impl BlockKind {
    pub fn is_solid(self) -> bool {
        !matches!(self, BlockKind::Spawn | BlockKind::Water)
    }

    pub fn is_one_way(self) -> bool {
        matches!(self, BlockKind::OneWay)
    }

    /// Whether this block's solidity depends on the global on/off channel.
    pub fn is_pulse(self) -> bool {
        matches!(self, BlockKind::TimedPulse)
    }

    pub fn is_conveyor(self) -> bool {
        matches!(
            self,
            BlockKind::Conveyor
                | BlockKind::ThinConveyor
                | BlockKind::OnOffConveyorA
                | BlockKind::OnOffConveyorB
        )
    }

    /// Renders and collides as a thin top slab.
    pub fn is_thin(self) -> bool {
        matches!(
            self,
            BlockKind::ThinConveyor | BlockKind::HangRail | BlockKind::OneWay
        )
    }

    /// Whether an on/off conveyor with this kind pushes while the switch is on.
    pub fn conveyor_active(self, onoff: bool) -> bool {
        match self {
            BlockKind::OnOffConveyorA => onoff,
            BlockKind::OnOffConveyorB => !onoff,
            _ => self.is_conveyor(),
        }
    }

    /// Whether the underside of this block is grab-able (hang verb).
    pub fn has_hangable_underside(self) -> bool {
        matches!(
            self,
            BlockKind::ThinConveyor
                | BlockKind::OnOffConveyorA
                | BlockKind::OnOffConveyorB
                | BlockKind::HangRail
        )
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
            BlockKind::ThinConveyor => "Thin Conveyor",
            BlockKind::OnOffConveyorA => "On/Off Conveyor A",
            BlockKind::OnOffConveyorB => "On/Off Conveyor B",
            BlockKind::HangRail => "Hang Rail",
            BlockKind::OneWay => "One-Way",
            BlockKind::TimedPulse => "Timed Pulse",
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
    BlockKind::ThinConveyor,
    BlockKind::OnOffConveyorA,
    BlockKind::OnOffConveyorB,
    BlockKind::HangRail,
    BlockKind::OneWay,
    BlockKind::TimedPulse,
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
    /// Thin top slab: 1x0.16x1 sitting at the top of the cell, with a crawl
    /// space beneath (used by thin conveyors and hang rails).
    Thin,
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
            BlockShape::Thin => "Thin",
        }
    }
}

/// Thickness (fraction of a cell) of thin top slabs: thin conveyors and hang
/// rails. Leaves a crawl space underneath.
pub const THIN_HEIGHT: f32 = 0.16;

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
    BlockShape::Thin,
];
