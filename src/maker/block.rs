pub use rustbox_format::block::{ALL_BLOCK_SHAPES, BlockKind, BlockShape};

pub trait BlockKindColor {
    fn color(&self) -> [f32; 3];
}

impl BlockKindColor for BlockKind {
    fn color(&self) -> [f32; 3] {
        match self {
            BlockKind::Grass => [0.35, 0.72, 0.35],
            BlockKind::Stone => [0.55, 0.55, 0.60],
            BlockKind::Hazard => [0.85, 0.20, 0.20],
            BlockKind::Goal => [0.95, 0.82, 0.25],
            BlockKind::Spawn => [0.25, 0.55, 0.95],
            BlockKind::Water => [0.20, 0.55, 0.95],
            BlockKind::Ice => [0.65, 0.85, 0.95],
            BlockKind::Spikes => [0.55, 0.55, 0.62],
            BlockKind::Conveyor => [0.45, 0.75, 0.95],
            BlockKind::Bounce => [0.95, 0.45, 0.45],
            BlockKind::Climb => [0.45, 0.65, 0.45],
            BlockKind::ThinConveyor => [0.45, 0.80, 0.95],
            BlockKind::OnOffConveyorA => [0.45, 0.75, 0.95],
            BlockKind::OnOffConveyorB => [0.40, 0.65, 0.90],
            BlockKind::HangRail => [0.55, 0.55, 0.55],
            BlockKind::OneWay => [0.20, 0.75, 0.75],
            BlockKind::TimedPulse => [0.95, 0.60, 0.20],
        }
    }
}
