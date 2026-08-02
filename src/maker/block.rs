use bevy::prelude::*;

pub use rustbox_format::block::{ALL_BLOCK_SHAPES, BlockKind, BlockShape};

/// Bevy-side color for a block kind (rendering, cursor previews, thumbnails).
pub trait BlockKindColor {
    fn color(&self) -> Color;
}

impl BlockKindColor for BlockKind {
    fn color(&self) -> Color {
        match self {
            BlockKind::Grass => Color::srgb(0.35, 0.72, 0.35),
            BlockKind::Stone => Color::srgb(0.55, 0.55, 0.60),
            BlockKind::Hazard => Color::srgb(0.85, 0.20, 0.20),
            BlockKind::Goal => Color::srgb(0.95, 0.82, 0.25),
            BlockKind::Spawn => Color::srgb(0.25, 0.55, 0.95),
            BlockKind::Water => Color::srgb(0.20, 0.55, 0.95),
        }
    }
}
