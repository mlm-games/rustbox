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
    Teleporter,
    Fan,
    Bumper,
    Crate,
    Key,
    LockGate,
    HealOrb,
    SpeedRing,
    CrumblePlate,
    Cannon,
    /// Stand-alone switch that toggles on/off when the player touches it.
    OnOffSwitch,
    /// A physics crate the player can pick up and throw (commit 20).
    TossCrate,
    /// A readable wooden signpost that shows its `sign_text` in a dialog when
    /// the player interacts with it.
    Sign,
    /// A 45° wedge/ramp solid. Passive: walk up the slope, slide down the
    /// steep side.
    Wedge,
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
            Self::Teleporter => "Teleporter",
            Self::Fan => "Fan",
            Self::Bumper => "Bumper",
            Self::Crate => "Crate",
            Self::Key => "Key",
            Self::LockGate => "Lock Gate",
            Self::HealOrb => "Heal Orb",
            Self::SpeedRing => "Speed Ring",
            Self::CrumblePlate => "Crumble Plate",
            Self::Cannon => "Cannon",
            Self::OnOffSwitch => "On/Off Switch",
            Self::TossCrate => "Toss Crate",
            Self::Sign => "Sign",
            Self::Wedge => "Wedge",
        }
    }

    /// Shown in the entity swatch bar / needs link channel UI.
    pub fn uses_link(self) -> bool {
        matches!(
            self,
            Self::TriggerOrb | Self::RelayGate | Self::Teleporter | Self::Key | Self::LockGate
        )
    }

    /// Hide meaningless param steppers in the inspector.
    pub fn has_param(self) -> bool {
        !matches!(self, Self::Checkpoint | Self::Key | Self::Sign)
    }

    /// Kinds that can hold a `ContainedItem` (v6+).
    pub fn supports_contents(self) -> bool {
        matches!(self, Self::Crate | Self::Prowler)
    }

    /// Kinds that may share a cell with others of the same kind (stacking).
    pub fn stackable(self) -> bool {
        matches!(
            self,
            Self::Glimmer | Self::TriggerOrb | Self::RelayGate | Self::Crate | Self::TossCrate
        )
    }

    /// Max number of same-kind entities that can share one cell.
    pub fn max_stack(self) -> usize {
        match self {
            Self::Glimmer => 8,
            Self::TriggerOrb => 4,
            Self::RelayGate => 4,
            Self::Crate => 4,
            Self::TossCrate => 4,
            _ => 1,
        }
    }
}

/// What pops out of a container when it is broken (Crate) or defeated
/// (Prowler). v6+. A contained Key uses the **container's** link channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ContainedItem {
    #[default]
    None,
    Glimmers(u8),
    Key,
    HealOrb,
    SpeedRing,
}

impl ContainedItem {
    pub fn label(self) -> String {
        match self {
            Self::None => "Empty".to_string(),
            Self::Glimmers(n) => format!("{n} Glimmers"),
            Self::Key => "Key".to_string(),
            Self::HealOrb => "Heal Orb".to_string(),
            Self::SpeedRing => "Speed Ring".to_string(),
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
    /// v6+: item inside this container. Always `None` for non-containers.
    #[serde(default)]
    pub contents: ContainedItem,
    /// v8+: text shown when the player reads a `Sign`. `\n` splits lines.
    #[serde(default)]
    pub sign_text: String,
}

fn default_param() -> f32 {
    1.0
}
