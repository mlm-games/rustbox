use bevy_ecs::prelude::{Component, Resource};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::collision::solid_floor_normal;
use super::entity_data::LevelEntityId;

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

/// A runtime dynamic solid (gate, seal, crate, crumble plate, wedge) used for
/// collision. `rotation` keeps rotated visuals physically aligned; the shape
/// (box or wedge) comes from the asset manifest, never from a render mesh.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeSolid {
    pub owner: LevelEntityId,
    pub center: Vec3,
    pub shape: SolidShape,
    pub rotation: Quat,
}

#[derive(Resource, Default)]
pub struct RuntimeSolids {
    pub solids: Vec<RuntimeSolid>,
}

impl RuntimeSolids {
    /// Floor normal underfoot when standing on an entity wedge (flat solids
    /// report `Vec3::Y`).
    pub fn floor_normal(&self, wx: f32, wz: f32) -> Vec3 {
        self.solids
            .iter()
            .filter_map(|s| solid_floor_normal(s, wx, wz))
            .find(|n| n.y < 0.999)
            .unwrap_or(Vec3::Y)
    }
}

#[derive(Component)]
pub struct DriftPlate {
    pub a: Vec3,
    pub b: Vec3,
    pub period: f32,
    pub t: f32,
    pub carry: Vec3,
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Velocity {
    pub linear: Vec3,
    pub angular: Vec3,
}

impl Velocity {
    pub fn zero() -> Self {
        Self {
            linear: Vec3::ZERO,
            angular: Vec3::ZERO,
        }
    }
}
