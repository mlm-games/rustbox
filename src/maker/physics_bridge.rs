//! Unified physics bridge: one query, one resolve.
//!
//! The game currently integrates the player with a custom mover and props
//! with a separate rigid-body engine, reconciled by rebuilding a solid table
//! several times per frame. This module implements the shared
//! `rustbox_physics::VoxelQuery` for `LevelDocument` so both sides can use
//! the same `move_and_collide` + `pushback` + `PhysicsState{ground_vel}`.
//!
//! Incremental adoption: new code calls `move_body`; old systems keep working
//! until migrated one by one.

use bevy::prelude::*;

use rustbox_physics::{
    Body, MoveResult, PhysicsState, Vec3 as PVec3, VoxelQuery, move_and_collide,
};

use super::collision;
use super::level::LevelDocument;

/// Adapter: `LevelDocument` as a voxel query (pulse-aware solidity, shaped
/// tops, conveyor ground velocity).
pub struct LevelQuery<'a> {
    pub level: &'a LevelDocument,
    /// Extra runtime solids (crates, seals, plates) sampled in world coords.
    /// `None` = level only.
    pub extra_solid: Option<&'a dyn Fn(IVec3) -> bool>,
}

impl VoxelQuery for LevelQuery<'_> {
    fn is_solid(&self, cell: [i32; 3]) -> bool {
        let iv = IVec3::from_array(cell);
        if self.level.is_solid(iv) {
            return true;
        }
        if let Some(f) = self.extra_solid {
            return f(iv);
        }
        false
    }

    fn surface_top(&self, cell: [i32; 3], wx: f32, wz: f32) -> Option<f32> {
        let iv = IVec3::from_array(cell);
        self.level
            .get_block(iv)
            .and_then(|b| collision::surface_top_height_opt(b, wx, wz))
    }

    fn ground_velocity(&self, cell: [i32; 3]) -> [f32; 3] {
        let iv = IVec3::from_array(cell);
        if let Some(b) = self.level.get_block(iv) {
            use rustbox_format::BlockKind;
            match b.kind {
                BlockKind::Conveyor | BlockKind::ThinConveyor => {
                    let dir = match b.rot % 4 {
                        0 => [1.0, 0.0, 0.0],
                        1 => [0.0, 0.0, 1.0],
                        2 => [-1.0, 0.0, 0.0],
                        _ => [0.0, 0.0, -1.0],
                    };
                    return [dir[0] * 3.0, 0.0, dir[2] * 3.0];
                }
                _ => {}
            }
        }
        [0.0, 0.0, 0.0]
    }
}

/// Move an AABB body by `delta` against the level. `pos` is the Bevy
/// `Transform` center; the shared mover works on AABB-min, so convert on
/// entry/exit. Returns the new center plus resolve result.
pub fn move_body(
    level: &LevelDocument,
    pos: Vec3,
    half_extents: Vec3,
    delta: Vec3,
    state: &mut PhysicsState,
    extra_solid: Option<&dyn Fn(IVec3) -> bool>,
) -> (Vec3, MoveResult) {
    let query = LevelQuery { level, extra_solid };
    let mut body = Body {
        pos: PVec3::new(
            pos.x - half_extents.x,
            pos.y - half_extents.y,
            pos.z - half_extents.z,
        ),
        half_extents: PVec3::new(half_extents.x, half_extents.y, half_extents.z),
        vel: PVec3::ZERO,
        on_ground: state.on_ground,
    };
    let result = move_and_collide(
        &query,
        &mut body,
        PVec3::new(delta.x, delta.y, delta.z),
        state,
    );
    (
        Vec3::new(
            body.pos.x + half_extents.x,
            body.pos.y + half_extents.y,
            body.pos.z + half_extents.z,
        ),
        result,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_falls_onto_floor() {
        let mut level = LevelDocument::default();
        let mut state = PhysicsState::default();
        let (pos, _) = move_body(
            &level,
            Vec3::new(0.0, 5.0, 0.0),
            Vec3::new(0.3, 0.9, 0.3),
            Vec3::new(0.0, -4.2, 0.0),
            &mut state,
            None,
        );
        assert!(pos.y > -1.0, "tunneled: y={}", pos.y);
        let _ = &mut level;
    }
}
