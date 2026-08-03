use serde::{Deserialize, Serialize};

use crate::track::TrackId;

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
    Checkpoint,
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
            Self::Checkpoint => "Checkpoint",
        }
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
