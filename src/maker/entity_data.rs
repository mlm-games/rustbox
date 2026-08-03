use bevy::prelude::*;

pub use rustbox_format::entity::{EntityData, EntityKind, LevelEntityId};

/// Bevy-side color for an entity kind (materials, gizmos, thumbnails).
pub trait EntityKindColor {
    fn color(&self) -> Color;
}

impl EntityKindColor for EntityKind {
    fn color(&self) -> Color {
        match self {
            Self::Glimmer => Color::srgb(1.0, 0.85, 0.25),
            Self::LaunchPad => Color::srgb(0.35, 0.75, 1.0),
            Self::Seal => Color::srgb(0.75, 0.35, 0.9),
            Self::DriftPlate => Color::srgb(0.95, 0.55, 0.25),
            Self::Prowler => Color::srgb(0.85, 0.2, 0.35),
            Self::TriggerOrb => Color::srgb(0.3, 0.9, 0.75),
            Self::RelayGate => Color::srgb(0.45, 0.85, 0.45),
            Self::Checkpoint => Color::srgb(0.95, 0.95, 1.0),
            Self::Teleporter => Color::srgb(0.55, 0.35, 1.0),
            Self::Fan => Color::srgb(0.65, 0.85, 1.0),
            Self::Bumper => Color::srgb(1.0, 0.45, 0.75),
            Self::Crate => Color::srgb(0.72, 0.5, 0.28),
            Self::Key => Color::srgb(1.0, 0.84, 0.2),
            Self::LockGate => Color::srgb(0.55, 0.55, 0.65),
            Self::HealOrb => Color::srgb(1.0, 0.35, 0.45),
            Self::SpeedRing => Color::srgb(0.2, 0.95, 0.55),
            Self::CrumblePlate => Color::srgb(0.7, 0.65, 0.55),
            Self::Cannon => Color::srgb(0.4, 0.45, 0.5),
        }
    }
}

/// Link channel color (1-9). Channel 0 (unlinked) = grey.
pub fn link_color(channel: u32) -> Color {
    match channel {
        1 => Color::srgb(0.95, 0.35, 0.35),
        2 => Color::srgb(0.35, 0.65, 0.95),
        3 => Color::srgb(0.95, 0.85, 0.35),
        4 => Color::srgb(0.45, 0.9, 0.45),
        5 => Color::srgb(0.85, 0.45, 0.95),
        6 => Color::srgb(0.95, 0.6, 0.3),
        7 => Color::srgb(0.4, 0.9, 0.9),
        8 => Color::srgb(0.95, 0.5, 0.75),
        9 => Color::srgb(0.8, 0.8, 0.8),
        _ => Color::srgb(0.5, 0.5, 0.5),
    }
}

/// Bevy-math helpers for entity data (the pure `EntityData` struct lives in
/// `rustbox-format`; these need `IVec3`).
pub trait EntityDataExt {
    fn cell_i(&self) -> IVec3;
    fn cell_b_i(&self) -> Option<IVec3>;
    fn defaults_for(kind: EntityKind, cell: IVec3, id: LevelEntityId) -> EntityData;
}

impl EntityDataExt for EntityData {
    fn cell_i(&self) -> IVec3 {
        IVec3::from_array(self.cell)
    }

    fn cell_b_i(&self) -> Option<IVec3> {
        self.cell_b.map(IVec3::from_array)
    }

    fn defaults_for(kind: EntityKind, cell: IVec3, id: LevelEntityId) -> EntityData {
        // cell_b is the legacy ping-pong path, kept only for migration of old
        // saves; new plates get their motion from a track.
        let (param, cell_b) = match kind {
            EntityKind::Glimmer => (1.0, None),
            EntityKind::LaunchPad => (14.0, None),
            EntityKind::Seal => (3.0, None),
            EntityKind::DriftPlate => (3.0, None),
            EntityKind::Prowler => (2.5, None),
            EntityKind::TriggerOrb => (1.0, None),
            EntityKind::RelayGate => (3.0, None),
            EntityKind::Checkpoint => (0.0, None),
            // cooldown seconds
            EntityKind::Teleporter => (0.6, None),
            // wind strength
            EntityKind::Fan => (12.0, None),
            // knockback strength
            EntityKind::Bumper => (16.0, None),
            // 1.0 = breakable by stomp/slam
            EntityKind::Crate => (1.0, None),
            EntityKind::Key => (0.0, None),
            // stays open this many seconds (0 = permanent until leave+reenter level)
            EntityKind::LockGate => (0.0, None),
            // heal amount (lives or HP units — mapped below)
            EntityKind::HealOrb => (1.0, None),
            // boost duration seconds
            EntityKind::SpeedRing => (2.5, None),
            // seconds before crumble after step
            EntityKind::CrumblePlate => (0.85, None),
            // arc height; cell_b is the target cell
            EntityKind::Cannon => (6.0, None),
        };
        EntityData {
            id,
            kind,
            cell: cell.to_array(),
            yaw_deg: 0.0,
            param,
            cell_b,
            track: None,
            link: 0,
        }
    }
}
