use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::track::TrackId;

pub type LevelEntityId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EntityKind {
    #[default]
    Glimmer,
    LaunchPad,
    Seal,
    DriftPlate,
    Prowler,
    TriggerOrb,
    RelayGate,
}

impl EntityKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Glimmer => "Glimmer",
            Self::LaunchPad => "Launch Pad",
            Self::Seal => "Seal",
            Self::DriftPlate => "Drift Plate",
            Self::Prowler => "Prowler",
            Self::TriggerOrb => "Trigger Orb",
            Self::RelayGate => "Relay Gate",
        }
    }

    pub fn color(self) -> Color {
        match self {
            Self::Glimmer => Color::srgb(1.0, 0.85, 0.25),
            Self::LaunchPad => Color::srgb(0.35, 0.75, 1.0),
            Self::Seal => Color::srgb(0.75, 0.35, 0.9),
            Self::DriftPlate => Color::srgb(0.95, 0.55, 0.25),
            Self::Prowler => Color::srgb(0.85, 0.2, 0.35),
            Self::TriggerOrb => Color::srgb(0.3, 0.9, 0.75),
            Self::RelayGate => Color::srgb(0.45, 0.85, 0.45),
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityData {
    pub id: LevelEntityId,
    pub kind: EntityKind,
    pub cell: [i32; 3],
    #[serde(default)]
    pub yaw_deg: f32,
    #[serde(default = "default_param")]
    pub param: f32,
    #[serde(default)]
    pub cell_b: Option<[i32; 3]>,
    #[serde(default)]
    pub track: Option<TrackId>,
    /// Link channel (1-9). 0 = unlinked.
    #[serde(default)]
    pub link: u32,
}

fn default_param() -> f32 {
    1.0
}

impl EntityData {
    pub fn cell_i(&self) -> IVec3 {
        IVec3::from_array(self.cell)
    }

    pub fn cell_b_i(&self) -> Option<IVec3> {
        self.cell_b.map(IVec3::from_array)
    }

    pub fn defaults_for(kind: EntityKind, cell: IVec3, id: LevelEntityId) -> Self {
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
        };
        Self {
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
