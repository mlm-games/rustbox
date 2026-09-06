//! Single-authority character physics.
//!
//! rustbox runs *two* physics truths — a custom AABB mover for the player
//! plus Rapier rigid bodies for crates/pads, bridged ad-hoc via
//! a rebuilt solid table 3x/frame and stale-velocity reads.
//! This crate provides the shared vocabulary so both sides query the *same*
//! [`VoxelQuery`] and integrate in *one* staged pass:
//!
//! ```text
//! target order: reset → maintain_pushback_cache → apply_pushback
//!   → handle_movement_and_terrain → update_cached_grid
//! game schedule: FixedUpdate.chain(rebuild_solids_once, apply_pushback,
//!   integrate_player_and_crates, write_back)
//! ```
//!
//! Pure Rust, no Bevy/Rapier dependency. The game crate implements
//! [`VoxelQuery`] for `LevelDocument` (+ runtime solid table) and converts
//! external colliders into [`Collider`] for the unified resolve.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    pub fn from_array(a: [f32; 3]) -> Self {
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
        }
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

/// Position + velocity split, so forces integrate before
/// collision writes back — no direct `Transform` mutation mid-pass.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub pos: Vec3,
    /// Feet position (pos is the AABB min corner).
    pub half_extents: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
}

impl Body {
    pub fn aabb(&self) -> (Vec3, Vec3) {
        let min = self.pos;
        let max = Vec3::new(
            self.pos.x + self.half_extents.x * 2.0,
            self.pos.y + self.half_extents.y * 2.0,
            self.pos.z + self.half_extents.z * 2.0,
        );
        (min, max)
    }
}

/// Last-resolve contact state: what did the last resolve touch, and how fast
/// was the ground moving (conveyor/plate ride without stale-velocity hacks).
#[derive(Clone, Debug, Default)]
pub struct PhysicsState {
    pub on_ground: bool,
    pub on_ceiling: bool,
    pub on_wall: Option<[f32; 3]>,
    pub in_fluid: bool,
    /// Velocity of the ground/platform underfoot, sampled during resolve.
    pub ground_vel: [f32; 3],
}

/// Collision shapes: boxes plus wedges (ramps). Mesh colliders are
/// approximated by their rotated AABB — explicit instead of divergence
/// between movers.
#[derive(Clone, Debug)]
pub enum Collider {
    Box {
        half_extents: [f32; 3],
    },
    /// Ramp rising toward local +X (mirrors `BlockShape::Slope`).
    Wedge {
        half_extents: [f32; 3],
        rot: u8,
    },
}

/// Broadphase cache: swept sphere + neighborhood radius so entity-entity
/// pushback can early-out.
#[derive(Clone, Debug, Default)]
pub struct PreviousPhysCache {
    pub velocity_dt: [f32; 3],
    pub collision_boundary: f32,
    pub neighborhood_radius: f32,
}

impl PreviousPhysCache {
    pub fn maintain(&mut self, vel: [f32; 3], dt: f32, radius: f32) {
        self.velocity_dt = [vel[0] * dt, vel[1] * dt, vel[2] * dt];
        let swept = (self.velocity_dt[0].powi(2)
            + self.velocity_dt[1].powi(2)
            + self.velocity_dt[2].powi(2))
        .sqrt();
        self.collision_boundary = swept + radius;
        self.neighborhood_radius = radius;
    }
}

/// World solidity query shared by player *and* props (the single truth that
/// replaces `RuntimeSolid` table + Rapier `Collider` divergence).
pub trait VoxelQuery {
    /// Is `cell` solid for movement? (Pulse/on-off already resolved by caller.)
    fn is_solid(&self, cell: [i32; 3]) -> bool;
    /// Top surface height of `cell` at world (wx, wz), if any material.
    fn surface_top(&self, cell: [i32; 3], wx: f32, wz: f32) -> Option<f32>;
    /// Ground/platform velocity at `cell` (conveyors, drift plates).
    fn ground_velocity(&self, cell: [i32; 3]) -> [f32; 3] {
        let _ = cell;
        [0.0, 0.0, 0.0]
    }
}

#[derive(Clone, Debug, Default)]
pub struct MoveResult {
    pub hit_x: bool,
    pub hit_y: bool,
    pub hit_z: bool,
    pub grounded: bool,
    pub ground_vel: [f32; 3],
}

pub const SKIN: f32 = 0.001;

/// Segmented axis-separated resolve: integrate in slices so fast bodies can't
/// tunnel through 1-cell walls.
pub fn move_and_collide<Q: VoxelQuery>(
    query: &Q,
    body: &mut Body,
    delta: Vec3,
    state: &mut PhysicsState,
) -> MoveResult {
    let mut result = MoveResult::default();
    let travel = (delta.x.abs() + delta.y.abs() + delta.z.abs()).max(0.0);
    let min_he = body
        .half_extents
        .x
        .min(body.half_extents.y)
        .min(body.half_extents.z)
        .max(0.05);
    let step_len = (min_he * 0.85).clamp(0.05, 0.3);
    let steps = ((travel / step_len).ceil() as usize).clamp(1, 100);

    state.on_ground = false;
    state.on_ceiling = false;
    state.on_wall = None;
    state.ground_vel = [0.0, 0.0, 0.0];

    for _ in 0..steps {
        let step = Vec3::new(
            delta.x / steps as f32,
            delta.y / steps as f32,
            delta.z / steps as f32,
        );
        resolve_axis(query, body, 0, step.x, &mut result, state);
        resolve_axis(query, body, 1, step.y, &mut result, state);
        resolve_axis(query, body, 2, step.z, &mut result, state);
    }

    body.on_ground = result.grounded;
    result
}

fn resolve_axis<Q: VoxelQuery>(
    query: &Q,
    body: &mut Body,
    axis: usize,
    amount: f32,
    result: &mut MoveResult,
    state: &mut PhysicsState,
) {
    if amount == 0.0 {
        return;
    }
    let pos = match axis {
        0 => &mut body.pos.x,
        1 => &mut body.pos.y,
        _ => &mut body.pos.z,
    };
    *pos += amount;

    let (min, max) = body.aabb();
    let x0 = (min.x + SKIN).floor() as i32;
    let x1 = (max.x - SKIN).floor() as i32;
    let y0 = (min.y + SKIN).floor() as i32;
    let y1 = (max.y - SKIN).floor() as i32;
    let z0 = (min.z + SKIN).floor() as i32;
    let z1 = (max.z - SKIN).floor() as i32;

    for cx in x0..=x1 {
        for cy in y0..=y1 {
            for cz in z0..=z1 {
                if !query.is_solid([cx, cy, cz]) {
                    continue;
                }
                match axis {
                    0 => {
                        if amount > 0.0 {
                            body.pos.x = cx as f32 - body.half_extents.x * 2.0 - SKIN;
                        } else {
                            body.pos.x = (cx + 1) as f32 + SKIN;
                        }
                        body.vel.x = 0.0;
                        result.hit_x = true;
                        state.on_wall = Some([-amount.signum(), 0.0, 0.0]);
                    }
                    1 => {
                        if amount > 0.0 {
                            body.pos.y = cy as f32 - body.half_extents.y * 2.0 - SKIN;
                            state.on_ceiling = true;
                        } else {
                            let wx = body.pos.x + body.half_extents.x;
                            let wz = body.pos.z + body.half_extents.z;
                            let top = query
                                .surface_top([cx, cy, cz], wx, wz)
                                .unwrap_or((cy + 1) as f32);
                            if body.pos.y >= top - 0.6 {
                                body.pos.y = top + SKIN;
                            } else {
                                body.pos.y = (cy + 1) as f32 + SKIN;
                            }
                            result.grounded = true;
                            state.on_ground = true;
                            state.ground_vel = query.ground_velocity([cx, cy, cz]);
                            result.ground_vel = state.ground_vel;
                        }
                        body.vel.y = 0.0;
                        result.hit_y = true;
                    }
                    _ => {
                        if amount > 0.0 {
                            body.pos.z = cz as f32 - body.half_extents.z * 2.0 - SKIN;
                        } else {
                            body.pos.z = (cz + 1) as f32 + SKIN;
                        }
                        body.vel.z = 0.0;
                        result.hit_z = true;
                        state.on_wall = Some([0.0, 0.0, -amount.signum()]);
                    }
                }
                return;
            }
        }
    }
}

/// Entity-entity pushback: separate overlapping AABBs along the
/// min-penetration axis before the terrain pass. Pure function so crates
/// *and* the player share it.
pub fn pushback(a: &mut Body, b: &mut Body) {
    let (amin, amax) = a.aabb();
    let (bmin, bmax) = b.aabb();
    let ox = (amax.x.min(bmax.x) - amin.x.max(bmin.x)).max(0.0);
    let oy = (amax.y.min(bmax.y) - amin.y.max(bmin.y)).max(0.0);
    let oz = (amax.z.min(bmax.z) - amin.z.max(bmin.z)).max(0.0);
    if ox == 0.0 || oy == 0.0 || oz == 0.0 {
        return;
    }
    if ox <= oy && ox <= oz {
        let push = ox / 2.0 + SKIN;
        if a.pos.x < b.pos.x {
            a.pos.x -= push;
            b.pos.x += push;
        } else {
            a.pos.x += push;
            b.pos.x -= push;
        }
    } else if oy <= ox && oy <= oz {
        let push = oy / 2.0 + SKIN;
        if a.pos.y < b.pos.y {
            a.pos.y -= push;
            b.pos.y += push;
        } else {
            a.pos.y += push;
            b.pos.y -= push;
        }
    } else {
        let push = oz / 2.0 + SKIN;
        if a.pos.z < b.pos.z {
            a.pos.z -= push;
            b.pos.z += push;
        } else {
            a.pos.z += push;
            b.pos.z -= push;
        }
    }
}

/// Rotated box AABB (single implementation both movers share — replaces the
/// divergent `rotated_box_aabb` vs Rapier hull).
pub fn rotated_box_aabb(half: [f32; 3], rot: u8) -> [f32; 3] {
    match rot % 4 {
        1 | 3 => [half[2], half[1], half[0]],
        _ => half,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    struct Floor {
        solids: HashSet<[i32; 3]>,
    }

    impl VoxelQuery for Floor {
        fn is_solid(&self, cell: [i32; 3]) -> bool {
            self.solids.contains(&cell)
        }
        fn surface_top(&self, cell: [i32; 3], _wx: f32, _wz: f32) -> Option<f32> {
            self.solids.contains(&cell).then_some((cell[1] + 1) as f32)
        }
    }

    fn floor() -> Floor {
        let mut solids = HashSet::new();
        for x in -4..=4 {
            for z in -4..=4 {
                solids.insert([x, 0, z]);
            }
        }
        Floor { solids }
    }

    #[test]
    fn falls_and_grounds() {
        let q = floor();
        let mut body = Body {
            pos: Vec3::new(0.0, 3.0, 0.0),
            half_extents: Vec3::new(0.3, 0.9, 0.3),
            vel: Vec3::new(0.0, -5.0, 0.0),
            on_ground: false,
        };
        let mut state = PhysicsState::default();
        let delta = Vec3::new(0.0, -5.0 * (1.0 / 60.0), 0.0);
        for _ in 0..120 {
            let d = Vec3::new(0.0, -10.0 * (1.0 / 60.0), 0.0);
            move_and_collide(&q, &mut body, d, &mut state);
            if state.on_ground {
                break;
            }
            let _ = delta;
        }
        assert!(state.on_ground);
        assert!((body.pos.y - 1.0).abs() < 0.05, "y={}", body.pos.y);
    }

    #[test]
    fn wall_blocks() {
        struct Wall(Floor);
        impl VoxelQuery for Wall {
            fn is_solid(&self, cell: [i32; 3]) -> bool {
                cell == [1, 1, 0] || self.0.is_solid(cell)
            }
            fn surface_top(&self, cell: [i32; 3], wx: f32, wz: f32) -> Option<f32> {
                self.0.surface_top(cell, wx, wz)
            }
        }
        let q = Wall(floor());
        let mut body = Body {
            pos: Vec3::new(0.0, 1.0, 0.0),
            half_extents: Vec3::new(0.3, 0.9, 0.3),
            vel: Vec3::ZERO,
            on_ground: true,
        };
        let mut state = PhysicsState::default();
        move_and_collide(&q, &mut body, Vec3::new(5.0, 0.0, 0.0), &mut state);
        assert!(body.pos.x + 0.6 < 1.0 + 0.01, "x={}", body.pos.x);
    }

    #[test]
    fn pushback_separates() {
        let mut a = Body {
            pos: Vec3::new(0.0, 0.0, 0.0),
            half_extents: Vec3::new(0.5, 0.5, 0.5),
            vel: Vec3::ZERO,
            on_ground: false,
        };
        let mut b = Body {
            pos: Vec3::new(0.5, 0.0, 0.0),
            half_extents: Vec3::new(0.5, 0.5, 0.5),
            vel: Vec3::ZERO,
            on_ground: false,
        };
        pushback(&mut a, &mut b);
        let (amin, amax) = a.aabb();
        let (bmin, bmax) = b.aabb();
        assert!(amax.x <= bmin.x + 0.01 || bmax.x <= amin.x + 0.01);
    }
}
