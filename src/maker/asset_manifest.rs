//! Data-driven entity asset manifest.
//!
//! Maps each [`EntityKind`] to its visual glTF model, rapier collider
//! primitive, kinematic solid shape, and tint language. Editing
//! `assets/models/entities.ron` swaps an asset pack without touching the
//! interaction kernel. If the file is missing or unparseable the hardcoded
//! [`EntityModelManifest::defaults`] are used, so it can be tested without them (if needed, for eg: on low-end devices).

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use rustbox_format::EntityKind;
use serde::{Deserialize, Serialize};

/// Collider primitive for rapier spawn (mirrors `bevy_rapier3d` shapes).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum ColliderPrimitive {
    /// No physics collider; the entity interacts via sensor volumes / AABBs.
    Sensor,
    /// `Collider::cuboid(hx, hy, hz)`.
    Box(f32, f32, f32),
    /// `Collider::cylinder(radius, height)`.
    Cylinder(f32, f32),
    /// Sloped-top triangular prism; `(hx, hy, hz)` is its bounding box.
    Wedge(f32, f32, f32),
}

/// Kinematic solid shape used by the hand-rolled collision engine.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum SolidShape {
    /// Full box of the given half-extents.
    Box(f32, f32, f32),
    /// 45° wedge filling its bounding box; the slope rises along the local
    /// +X axis from the low (hinge) edge to the tall edge.
    Wedge(f32, f32, f32),
}

impl SolidShape {
    /// Axis-aligned half-extents of the shape's bounding box.
    pub fn half_extents(self) -> Vec3 {
        match self {
            Self::Box(x, y, z) | Self::Wedge(x, y, z) => Vec3::new(x, y, z),
        }
    }

    pub fn is_wedge(self) -> bool {
        matches!(self, Self::Wedge(..))
    }
}

/// How a glTF model's materials are (re)tinted.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum TintMode {
    /// Flat `EntityKind::color()` material (collectible color language).
    Kind,
    /// Link-channel color (1-9).
    Link,
    /// Leave the model's own albedo textures / materials untouched.
    Model,
}

/// Visual + collision configuration for one entity kind.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityAssetEntry {
    /// glTF scene path loaded via the `AssetServer` (e.g.
    /// `models/pack/Seal.gltf#Scene0`). `None` = procedural mesh.
    pub model: Option<String>,
    /// Visual scale applied to the spawned model root.
    pub scale: f32,
    /// Vertical offset applied to the spawned model root.
    pub y_offset: f32,
    pub tint: TintMode,
    /// Rapier collider spawned in play mode. `None` = no collider.
    pub collider: Option<ColliderPrimitive>,
    /// Kinematic solid (or `None` for non-solid kinds like sensors/pickups).
    pub solid: Option<SolidShape>,
    /// Palette preview PNG path under `assets/` (e.g.
    /// `images/previews/entities/glimmer.png`). `None` = the per-kind default
    /// PNG (`images/entities/{kind}.png`).
    #[serde(default)]
    pub preview: Option<String>,
}

/// Full per-kind manifest. Keyed by `EntityKind` name (`"Seal"`, `"Wedge"`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityModelManifest {
    pub entities: HashMap<String, EntityAssetEntry>,
}

impl Default for EntityModelManifest {
    fn default() -> Self {
        Self::defaults()
    }
}

impl EntityModelManifest {
    pub fn entry(&self, kind: EntityKind) -> Option<&EntityAssetEntry> {
        self.entities.get(kind_name(kind))
    }

    pub fn solid(&self, kind: EntityKind) -> Option<SolidShape> {
        self.entry(kind).and_then(|e| e.solid)
    }

    pub fn collider(&self, kind: EntityKind) -> Option<ColliderPrimitive> {
        self.entry(kind).and_then(|e| e.collider)
    }

    /// Palette preview path for a kind, falling back to `images/entities/*.png`.
    pub fn preview(&self, kind: EntityKind) -> String {
        self.entry(kind)
            .and_then(|e| e.preview.clone())
            .unwrap_or_else(|| format!("images/entities/{}.png", preview_base(kind)))
    }

    /// Load the manifest from disk, falling back to the built-in defaults when
    /// the file is missing or malformed (the game must run either way).
    pub fn load(asset_root: &Path) -> Self {
        let path = asset_root.join("models/entities.ron");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_else(Self::defaults)
    }

    /// Built-in manifest (mirrors the historical hardcoded values). The RON
    /// file on disk overrides these per kind; entries absent from the file
    /// keep their defaults, so a partial pack swap stays safe.
    pub fn defaults() -> Self {
        let entry = |model: Option<&str>,
                     scale: f32,
                     y_offset: f32,
                     tint: TintMode,
                     collider: Option<ColliderPrimitive>,
                     solid: Option<SolidShape>| {
            EntityAssetEntry {
                model: model.map(ToOwned::to_owned),
                scale,
                y_offset,
                tint,
                collider,
                solid,
                preview: None,
            }
        };
        let mut m = HashMap::new();
        m.insert(
            kind_name(EntityKind::Glimmer).to_string(),
            entry(
                Some("models/pack/Glimmer.gltf#Scene0"),
                0.11,
                -0.20,
                TintMode::Kind,
                Some(ColliderPrimitive::Sensor),
                None,
            ),
        );
        m.insert(
            kind_name(EntityKind::LaunchPad).to_string(),
            entry(
                None,
                1.0,
                0.0,
                TintMode::Kind,
                Some(ColliderPrimitive::Cylinder(0.15, 0.45)),
                None,
            ),
        );
        m.insert(
            kind_name(EntityKind::Seal).to_string(),
            entry(
                Some("models/pack/Seal.gltf#Scene0"),
                0.5,
                -1.0,
                TintMode::Model,
                Some(ColliderPrimitive::Box(0.35, 0.35, 0.35)),
                Some(SolidShape::Box(0.5, 1.0, 0.15)),
            ),
        );
        m.insert(
            kind_name(EntityKind::DriftPlate).to_string(),
            entry(
                Some("models/pack/DriftPlate.gltf#Scene0"),
                0.8,
                -0.18,
                TintMode::Model,
                Some(ColliderPrimitive::Box(0.7, 0.12, 0.7)),
                None,
            ),
        );
        m.insert(
            kind_name(EntityKind::Prowler).to_string(),
            entry(
                Some("models/pack/Prowler.gltf#Scene0"),
                0.34,
                -0.4,
                TintMode::Model,
                Some(ColliderPrimitive::Sensor),
                None,
            ),
        );
        m.insert(
            kind_name(EntityKind::TriggerOrb).to_string(),
            entry(
                Some("models/pack/TriggerOrb.gltf#Scene0"),
                0.8,
                -1.0,
                TintMode::Link,
                Some(ColliderPrimitive::Sensor),
                None,
            ),
        );
        m.insert(
            kind_name(EntityKind::RelayGate).to_string(),
            entry(
                Some("models/pack/RelayGate.gltf#Scene0"),
                0.5,
                -1.0,
                TintMode::Link,
                Some(ColliderPrimitive::Box(0.5, 1.0, 0.2)),
                Some(SolidShape::Box(0.5, 1.0, 0.2)),
            ),
        );
        // Non-model kinds: sensors interact via AABB/trigger logic only.
        let sensor = |kind: EntityKind| {
            (
                kind_name(kind).to_string(),
                entry(
                    None,
                    1.0,
                    0.0,
                    TintMode::Kind,
                    Some(ColliderPrimitive::Sensor),
                    None,
                ),
            )
        };
        for kind in [
            EntityKind::Checkpoint,
            EntityKind::Teleporter,
            EntityKind::Fan,
            EntityKind::Bumper,
            EntityKind::Key,
            EntityKind::HealOrb,
            EntityKind::SpeedRing,
            EntityKind::Cannon,
            EntityKind::OnOffSwitch,
            EntityKind::Sign,
        ] {
            m.insert(sensor(kind).0, sensor(kind).1);
        }
        // Box-shaped solids with no physics collider.
        m.insert(
            kind_name(EntityKind::Crate).to_string(),
            entry(
                None,
                1.0,
                0.0,
                TintMode::Kind,
                None,
                Some(SolidShape::Box(0.5, 0.5, 0.5)),
            ),
        );
        m.insert(
            kind_name(EntityKind::TossCrate).to_string(),
            entry(
                None,
                1.0,
                0.0,
                TintMode::Kind,
                Some(ColliderPrimitive::Box(0.4, 0.4, 0.4)),
                Some(SolidShape::Box(0.5, 0.5, 0.5)),
            ),
        );
        m.insert(
            kind_name(EntityKind::LockGate).to_string(),
            entry(
                None,
                1.0,
                0.0,
                TintMode::Kind,
                None,
                Some(SolidShape::Box(0.55, 1.2, 0.3)),
            ),
        );
        m.insert(
            kind_name(EntityKind::CrumblePlate).to_string(),
            entry(
                None,
                1.0,
                0.0,
                TintMode::Kind,
                None,
                Some(SolidShape::Box(0.5, 0.12, 0.5)),
            ),
        );
        m.insert(
            kind_name(EntityKind::Wedge).to_string(),
            entry(
                None,
                1.0,
                0.0,
                TintMode::Kind,
                Some(ColliderPrimitive::Wedge(0.5, 0.5, 0.5)),
                Some(SolidShape::Wedge(0.5, 0.5, 0.5)),
            ),
        );
        Self { entities: m }
    }

    /// Merge entries from `other` (e.g. a RON override) over the defaults.
    /// Kinds present in `other` replace their default entry entirely.
    pub fn merged_with(mut self, other: Self) -> Self {
        self.entities.extend(other.entities);
        self
    }
}

/// Stable string key for a kind (the `EntityKind` variant name, which is also
/// what the RON uses). Kept here so the manifest never depends on UI labels.
pub fn kind_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Glimmer => "Glimmer",
        EntityKind::LaunchPad => "LaunchPad",
        EntityKind::Seal => "Seal",
        EntityKind::DriftPlate => "DriftPlate",
        EntityKind::Prowler => "Prowler",
        EntityKind::TriggerOrb => "TriggerOrb",
        EntityKind::RelayGate => "RelayGate",
        EntityKind::Checkpoint => "Checkpoint",
        EntityKind::Teleporter => "Teleporter",
        EntityKind::Fan => "Fan",
        EntityKind::Bumper => "Bumper",
        EntityKind::Crate => "Crate",
        EntityKind::Key => "Key",
        EntityKind::LockGate => "LockGate",
        EntityKind::HealOrb => "HealOrb",
        EntityKind::SpeedRing => "SpeedRing",
        EntityKind::CrumblePlate => "CrumblePlate",
        EntityKind::Cannon => "Cannon",
        EntityKind::OnOffSwitch => "OnOffSwitch",
        EntityKind::TossCrate => "TossCrate",
        EntityKind::Sign => "Sign",
        EntityKind::Wedge => "Wedge",
    }
}

/// Palette preview file base name for an entity kind (`"glimmer"`, ...).
/// Mirrors the names under `assets/images/entities/`.
pub fn preview_base(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Glimmer => "glimmer",
        EntityKind::LaunchPad => "launch_pad",
        EntityKind::Seal => "seal",
        EntityKind::DriftPlate => "drift_plate",
        EntityKind::Prowler => "prowler",
        EntityKind::TriggerOrb => "trigger_orb",
        EntityKind::RelayGate => "relay_gate",
        EntityKind::Checkpoint => "checkpoint",
        EntityKind::Teleporter => "teleporter",
        EntityKind::Fan => "fan",
        EntityKind::Bumper => "bumper",
        EntityKind::Crate => "crate",
        EntityKind::Key => "key",
        EntityKind::LockGate => "lock_gate",
        EntityKind::HealOrb => "heal_orb",
        EntityKind::SpeedRing => "speed_ring",
        EntityKind::CrumblePlate => "crumble_plate",
        EntityKind::Cannon => "cannon",
        EntityKind::OnOffSwitch => "on_off_switch",
        EntityKind::TossCrate => "toss_crate",
        EntityKind::Sign => "sign",
        EntityKind::Wedge => "wedge",
    }
}

/// Look up a kind from its manifest key, `None` for unknown names.
pub fn kind_from_name(name: &str) -> Option<EntityKind> {
    match name {
        "Glimmer" => Some(EntityKind::Glimmer),
        "LaunchPad" => Some(EntityKind::LaunchPad),
        "Seal" => Some(EntityKind::Seal),
        "DriftPlate" => Some(EntityKind::DriftPlate),
        "Prowler" => Some(EntityKind::Prowler),
        "TriggerOrb" => Some(EntityKind::TriggerOrb),
        "RelayGate" => Some(EntityKind::RelayGate),
        "Checkpoint" => Some(EntityKind::Checkpoint),
        "Teleporter" => Some(EntityKind::Teleporter),
        "Fan" => Some(EntityKind::Fan),
        "Bumper" => Some(EntityKind::Bumper),
        "Crate" => Some(EntityKind::Crate),
        "Key" => Some(EntityKind::Key),
        "LockGate" => Some(EntityKind::LockGate),
        "HealOrb" => Some(EntityKind::HealOrb),
        "SpeedRing" => Some(EntityKind::SpeedRing),
        "CrumblePlate" => Some(EntityKind::CrumblePlate),
        "Cannon" => Some(EntityKind::Cannon),
        "OnOffSwitch" => Some(EntityKind::OnOffSwitch),
        "TossCrate" => Some(EntityKind::TossCrate),
        "Sign" => Some(EntityKind::Sign),
        "Wedge" => Some(EntityKind::Wedge),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_every_kind() {
        let m = EntityModelManifest::defaults();
        for kind in [
            EntityKind::Glimmer,
            EntityKind::LaunchPad,
            EntityKind::Seal,
            EntityKind::DriftPlate,
            EntityKind::Prowler,
            EntityKind::TriggerOrb,
            EntityKind::RelayGate,
            EntityKind::Checkpoint,
            EntityKind::Teleporter,
            EntityKind::Fan,
            EntityKind::Bumper,
            EntityKind::Crate,
            EntityKind::Key,
            EntityKind::LockGate,
            EntityKind::HealOrb,
            EntityKind::SpeedRing,
            EntityKind::CrumblePlate,
            EntityKind::Cannon,
            EntityKind::OnOffSwitch,
            EntityKind::TossCrate,
            EntityKind::Sign,
            EntityKind::Wedge,
        ] {
            assert!(m.entry(kind).is_some(), "missing default for {kind:?}");
        }
    }

    #[test]
    fn name_roundtrip() {
        for kind in [
            EntityKind::Glimmer,
            EntityKind::LaunchPad,
            EntityKind::Seal,
            EntityKind::DriftPlate,
            EntityKind::Prowler,
            EntityKind::TriggerOrb,
            EntityKind::RelayGate,
            EntityKind::Checkpoint,
            EntityKind::Teleporter,
            EntityKind::Fan,
            EntityKind::Bumper,
            EntityKind::Crate,
            EntityKind::Key,
            EntityKind::LockGate,
            EntityKind::HealOrb,
            EntityKind::SpeedRing,
            EntityKind::CrumblePlate,
            EntityKind::Cannon,
            EntityKind::OnOffSwitch,
            EntityKind::TossCrate,
            EntityKind::Sign,
            EntityKind::Wedge,
        ] {
            assert_eq!(kind_from_name(kind_name(kind)), Some(kind));
        }
    }

    #[test]
    fn disk_manifest_parses_and_roundtrips_with_defaults() {
        // The shipped file must parse and (after a merge over defaults) cover
        // every kind, so a broken edit can never silently drop a kind.
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/models/entities.ron");
        let text = std::fs::read_to_string(&path).expect("manifest file exists");
        let disk: EntityModelManifest = ron::from_str(&text).expect("manifest parses");
        let merged = EntityModelManifest::defaults().merged_with(disk);
        for kind in [
            EntityKind::Glimmer,
            EntityKind::LaunchPad,
            EntityKind::Seal,
            EntityKind::DriftPlate,
            EntityKind::Prowler,
            EntityKind::TriggerOrb,
            EntityKind::RelayGate,
            EntityKind::Checkpoint,
            EntityKind::Teleporter,
            EntityKind::Fan,
            EntityKind::Bumper,
            EntityKind::Crate,
            EntityKind::Key,
            EntityKind::LockGate,
            EntityKind::HealOrb,
            EntityKind::SpeedRing,
            EntityKind::CrumblePlate,
            EntityKind::Cannon,
            EntityKind::OnOffSwitch,
            EntityKind::TossCrate,
            EntityKind::Sign,
            EntityKind::Wedge,
        ] {
            assert!(merged.entry(kind).is_some(), "manifest lost {kind:?}");
        }
    }

    #[test]
    fn wedge_defaults_are_wedge_shaped() {
        let m = EntityModelManifest::defaults();
        let entry = m.entry(EntityKind::Wedge).unwrap();
        assert_eq!(entry.solid, Some(SolidShape::Wedge(0.5, 0.5, 0.5)));
        assert_eq!(
            entry.collider,
            Some(ColliderPrimitive::Wedge(0.5, 0.5, 0.5))
        );
        assert_eq!(
            SolidShape::Wedge(0.5, 0.5, 0.5).half_extents(),
            Vec3::splat(0.5)
        );
    }

    #[test]
    fn every_kind_has_a_preview_beneath_assets() {
        let m = EntityModelManifest::defaults();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for kind in [
            EntityKind::Glimmer,
            EntityKind::LaunchPad,
            EntityKind::Seal,
            EntityKind::DriftPlate,
            EntityKind::Prowler,
            EntityKind::TriggerOrb,
            EntityKind::RelayGate,
            EntityKind::Checkpoint,
            EntityKind::Teleporter,
            EntityKind::Fan,
            EntityKind::Bumper,
            EntityKind::Crate,
            EntityKind::Key,
            EntityKind::LockGate,
            EntityKind::HealOrb,
            EntityKind::SpeedRing,
            EntityKind::CrumblePlate,
            EntityKind::Cannon,
            EntityKind::OnOffSwitch,
            EntityKind::TossCrate,
            EntityKind::Sign,
            EntityKind::Wedge,
        ] {
            let p = root.join(m.preview(kind));
            assert!(p.exists(), "{}: preview missing at {}", kind.label(), p.display());
        }
    }
}
