use bevy::prelude::*;

use super::block::BlockKind;
use super::level::{BlockData, LevelDocument};
use rustbox_format::BlockShape;

pub fn is_solid(level: &LevelDocument, cell: IVec3) -> bool {
    level.is_solid(cell)
}

/// Local-space height of the top surface of a shape at local coords
/// `(lx, lz)` in [0,1]^2 (before rotation), in [0,1].
fn local_surface_height(shape: BlockShape, lx: f32, lz: f32) -> f32 {
    match shape {
        BlockShape::Full => 1.0,
        BlockShape::Half => 0.5,
        BlockShape::TopHalf => 1.0,
        BlockShape::Slope => lx,
        BlockShape::DSlope => 1.0 - lx,
        BlockShape::Corner => (lx + lz) * 0.5,
        BlockShape::OuterCorner => 1.0 - (lx + lz) * 0.5,
        BlockShape::VerticalSlope => 1.0,
        BlockShape::VerticalSlab => {
            if lz <= 0.5 {
                1.0
            } else {
                0.0
            }
        }
        BlockShape::Thin => 1.0,
    }
}

/// Whether the column at local coords `(lx, lz)` (before rotation) contains
/// any solid material (used so vertical snapping skips empty halves of
/// partial shapes).
fn local_column_solid(shape: BlockShape, lx: f32, lz: f32) -> bool {
    match shape {
        BlockShape::Full
        | BlockShape::Half
        | BlockShape::TopHalf
        | BlockShape::Slope
        | BlockShape::DSlope
        | BlockShape::Corner
        | BlockShape::OuterCorner => true,
        BlockShape::VerticalSlope => lx + lz <= 1.0,
        BlockShape::VerticalSlab => lz <= 0.5,
        BlockShape::Thin => true,
    }
}

/// Local-space height where solid material *starts* in a column (the block's
/// bottom surface). Used for upward collision so players can slide under
/// overhanging slabs instead of bumping the full cell.
fn local_bottom_height(shape: BlockShape, _lx: f32, _lz: f32) -> f32 {
    match shape {
        BlockShape::TopHalf => 0.5,
        BlockShape::Thin => 1.0 - rustbox_format::block::THIN_HEIGHT,
        _ => 0.0,
    }
}

/// World-space bottom-surface height of a block at world position (wx, wz).
/// Returns `None` when the column is empty (no material to hit).
pub fn surface_bottom_height(block: &BlockData, wx: f32, wz: f32) -> Option<f32> {
    let wx = (wx - block.position[0] as f32).clamp(0.0, 1.0);
    let wz = (wz - block.position[2] as f32).clamp(0.0, 1.0);
    let (lx, lz) = local_from_world(wx, wz, block.rot);
    let (lx, lz) = (lx.clamp(0.0, 1.0), lz.clamp(0.0, 1.0));
    if !local_column_solid(block.shape, lx, lz) {
        return None;
    }
    Some(block.position[1] as f32 + local_bottom_height(block.shape, lx, lz))
}

/// Rotate a point inside a cell (world-local, [0,1]) into the shape's
/// un-rotated local frame, given the block's yaw `rot` (0..3, 90deg steps).
fn local_from_world(wx: f32, wz: f32, rot: u8) -> (f32, f32) {
    let angle = rot as f32 * std::f32::consts::FRAC_PI_2;
    let (s, c) = angle.sin_cos();
    let sx = wx - 0.5;
    let sz = wz - 0.5;
    (c * sx - s * sz + 0.5, s * sx + c * sz + 0.5)
}

/// World-space top-surface height of a block at world position (wx, wz).
pub fn surface_top_height(block: &BlockData, wx: f32, wz: f32) -> f32 {
    let wx = (wx - block.position[0] as f32).clamp(0.0, 1.0);
    let wz = (wz - block.position[2] as f32).clamp(0.0, 1.0);
    let (lx, lz) = local_from_world(wx, wz, block.rot);
    block.position[1] as f32
        + local_surface_height(block.shape, lx.clamp(0.0, 1.0), lz.clamp(0.0, 1.0))
}

/// A horizontal direction expressed in the world frame, converted into the
/// shape's local frame so face-solidity can be looked up.
fn local_axis_direction(axis: usize, side: u8, rot: u8) -> IVec2 {
    let world = match (axis, side) {
        (0, 0) => IVec2::NEG_X,
        (0, 1) => IVec2::X,
        (2, 0) => IVec2::new(0, -1),
        (2, 1) => IVec2::new(0, 1),
        _ => IVec2::ZERO,
    };
    // Rotate the world direction into the local frame: for rot=1 (+90deg),
    // world +X maps to local -Z, etc.
    match rot % 4 {
        0 => world,
        1 => IVec2::new(world.y, -world.x),
        2 => -world,
        3 => IVec2::new(-world.y, world.x),
        _ => world,
    }
}

/// Local vertical extent `[lo, hi]` (in [0,1]) of solid material on a given
/// horizontal face. `axis` 0 = X faces, 2 = Z faces; `side` 0 = low, 1 = high.
fn face_solid_range(shape: BlockShape, rot: u8, axis: usize, side: u8) -> (f32, f32) {
    match shape {
        BlockShape::Full => (0.0, 1.0),
        BlockShape::Half => (0.0, 0.5),
        BlockShape::TopHalf => (0.5, 1.0),
        BlockShape::Thin => (1.0 - rustbox_format::block::THIN_HEIGHT, 1.0),
        BlockShape::VerticalSlab => {
            // Slab sits against the local -Z face; the +Z cell face is open.
            let local = local_axis_direction(axis, side, rot);
            if local == IVec2::new(0, 1) {
                (0.0, 0.0)
            } else {
                (0.0, 1.0)
            }
        }
        BlockShape::VerticalSlope => {
            // Solid quarter is against -X/-Z; +X and +Z faces are open edges.
            let local = local_axis_direction(axis, side, rot);
            if local == IVec2::X || local == IVec2::new(0, 1) {
                (0.0, 0.0)
            } else {
                (0.0, 1.0)
            }
        }
        _ => {
            let local = local_axis_direction(axis, side, rot);
            match shape {
                BlockShape::Slope => {
                    // Rises toward local +X. The tall end (+X face) is a full
                    // wall; the low end (-X face) is a zero-area edge; the
                    // +/-Z sides span the full height.
                    if local == IVec2::X {
                        (0.0, 1.0)
                    } else if local == IVec2::NEG_X {
                        (0.0, 0.0)
                    } else {
                        (0.0, 1.0)
                    }
                }
                BlockShape::DSlope => {
                    // Rises toward local -X: mirror of Slope.
                    if local == IVec2::NEG_X {
                        (0.0, 1.0)
                    } else if local == IVec2::X {
                        (0.0, 0.0)
                    } else {
                        (0.0, 1.0)
                    }
                }
                BlockShape::Corner => {
                    if local.x >= 0 && local.y >= 0 {
                        (0.5, 1.0)
                    } else {
                        (0.0, 0.5)
                    }
                }
                BlockShape::OuterCorner => {
                    if local.x >= 0 && local.y >= 0 {
                        (0.0, 0.5)
                    } else {
                        (0.5, 1.0)
                    }
                }
                _ => (0.0, 1.0),
            }
        }
    }
}

/// Does the vertical span `[lo, hi]` overlap `[flo, fhi]`?
fn vertical_overlap(lo: f32, hi: f32, flo: f32, fhi: f32) -> bool {
    hi - 0.001 > flo && lo + 0.001 < fhi
}

fn resolve_axis(
    pos: &mut Vec3,
    he: Vec3,
    delta: f32,
    axis: usize,
    level: &LevelDocument,
    extras: &[(Vec3, Vec3)],
) -> bool {
    if delta == 0.0 {
        return false;
    }

    let mut collided = false;

    let start_axis = pos[axis];
    let mut p = pos.to_array();
    p[axis] += delta;
    let he = he.to_array();

    let v = Vec3::from_array(p);
    let min = v - Vec3::from_array(he);
    let max = v + Vec3::from_array(he);

    for x in (min.x.floor() as i32)..=(max.x.floor() as i32) {
        for y in (min.y.floor() as i32)..=(max.y.floor() as i32) {
            for z in (min.z.floor() as i32)..=(max.z.floor() as i32) {
                let cell = IVec3::new(x, y, z);
                let block = level.get_block(cell);
                // Boundary cells with no block (or a non-solid block like a
                // spawn marker) are still solid, so invisible walls/floor
                // actually stop the player.
                let boundary =
                    block.map(|b| !b.kind.is_solid()).unwrap_or(true) && level.boundary_solid(cell);
                let solid = block.is_some_and(|b| level.kind_is_solid(b.kind)) || boundary;
                if !solid {
                    continue;
                }
                // One-way platforms are solid only when approached from above:
                // land on top, but pass through from below and from the sides.
                if block.is_some_and(|b| b.kind.is_one_way()) && (axis != 1 || delta > 0.0) {
                    continue;
                }

                if axis == 1 {
                    let moved = Vec3::from_array(p);
                    let amin = moved - Vec3::from_array(he);
                    let amax = moved + Vec3::from_array(he);
                    let cmin = cell.as_vec3();
                    let cmax = cmin + Vec3::ONE;
                    if !(amin.x < cmax.x
                        && amax.x > cmin.x
                        && amin.y < cmax.y
                        && amax.y > cmin.y
                        && amin.z < cmax.z
                        && amax.z > cmin.z)
                    {
                        continue;
                    }
                    // Vertical movement is special-cased so ramps/slabs give a
                    // shaped top surface instead of a flat cell top.
                    if delta < 0.0 {
                        let top = match block {
                            Some(b) if level.kind_is_solid(b.kind) => {
                                surface_top_height(b, v.x, v.z)
                            }
                            _ => cell.y as f32 + 1.0,
                        };
                        // Only snap when the column under the player is solid
                        // (skip the empty halves of partial shapes) and the
                        // player's bottom crosses the surface from above this
                        // frame (`prev_feet` = feet before the move).
                        let column_solid = match block {
                            Some(b) if level.kind_is_solid(b.kind) => {
                                let (lx, lz) = local_from_world(
                                    (v.x - cell.x as f32).clamp(0.0, 1.0),
                                    (v.z - cell.z as f32).clamp(0.0, 1.0),
                                    b.rot,
                                );
                                local_column_solid(b.shape, lx.clamp(0.0, 1.0), lz.clamp(0.0, 1.0))
                            }
                            _ => true,
                        };
                        let feet = p[1] - he[1];
                        let prev_feet = feet - delta;
                        const RIDE: f32 = 0.35;
                        if column_solid
                            && feet <= top + 0.001
                            && (prev_feet >= top - 0.001 || prev_feet >= top - RIDE)
                        {
                            p[1] = top + he[1];
                            collided = true;
                        }
                    } else {
                        // Moving up into a shaped block: only collide if the
                        // column has material, and stop just under the
                        // material's bottom surface so overhangs (TopHalf, thin
                        // slabs) leave crawl space.
                        let bottom = match block {
                            Some(b) if level.kind_is_solid(b.kind) => {
                                surface_bottom_height(b, v.x, v.z)
                            }
                            _ => Some(cell.y as f32),
                        };
                        let head = p[1] + he[1];
                        let prev_head = head - delta;
                        if let Some(bottom) = bottom
                            && bottom >= prev_head - 0.001
                            && head >= bottom - 0.001
                        {
                            p[1] = bottom - he[1];
                            collided = true;
                        }
                    }
                } else {
                    // Horizontal: only block if the player's vertical span
                    // overlaps the solid material on that face (slabs and
                    // slopes leave gaps a small player can slip under).
                    let (flo, fhi) = match block {
                        Some(b) if level.kind_is_solid(b.kind) => {
                            face_solid_range(b.shape, b.rot, axis, if delta > 0.0 { 0 } else { 1 })
                        }
                        _ => (0.0, 1.0),
                    };
                    let vlo = p[1] - he[1];
                    let vhi = p[1] + he[1];
                    if !vertical_overlap(vlo, vhi, cell.y as f32 + flo, cell.y as f32 + fhi) {
                        continue;
                    }
                    let moved = Vec3::from_array(p);
                    let amin = moved - Vec3::from_array(he);
                    let amax = moved + Vec3::from_array(he);
                    let cmin = cell.as_vec3();
                    let cmax = cmin + Vec3::ONE;
                    if !(amin.x < cmax.x
                        && amax.x > cmin.x
                        && amin.y < cmax.y
                        && amax.y > cmin.y
                        && amin.z < cmax.z
                        && amax.z > cmin.z)
                    {
                        continue;
                    }
                    let (leading, near_face) = if delta > 0.0 {
                        (start_axis + he[axis], cmin[axis])
                    } else {
                        (start_axis - he[axis], cmax[axis])
                    };
                    let ahead = if delta > 0.0 {
                        cmin[axis] >= leading
                    } else {
                        cmax[axis] <= leading
                    };
                    if !ahead {
                        continue;
                    }
                    let stop = if delta > 0.0 {
                        near_face - he[axis]
                    } else {
                        near_face + he[axis]
                    };
                    let closer = if delta > 0.0 {
                        stop < p[axis]
                    } else {
                        stop > p[axis]
                    };
                    if closer {
                        p[axis] = stop;
                    }
                    collided = true;
                }
            }
        }
    }

    // Extras (gates, seals, crates, plates) are axis-aligned boxes at
    // (`ec`, half-extent `ehe`). Resolve them as full AABBs so the player
    // stops flush against the face it moves into instead of sinking into the
    // box's center and getting slammed back out.
    for (ec, ehe) in extras {
        let ec = ec.to_array();
        let ehe = ehe.to_array();
        let bmin = [ec[0] - ehe[0], ec[1] - ehe[1], ec[2] - ehe[2]];
        let bmax = [ec[0] + ehe[0], ec[1] + ehe[1], ec[2] + ehe[2]];
        // Player AABB at the candidate position (half-extents, not a point).
        let amin = [p[0] - he[0], p[1] - he[1], p[2] - he[2]];
        let amax = [p[0] + he[0], p[1] + he[1], p[2] + he[2]];
        // Transverse axes must overlap the box; the moved axis is handled below.
        let (ta, tb) = match axis {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        if amin[ta] >= bmax[ta]
            || amax[ta] <= bmin[ta]
            || amin[tb] >= bmax[tb]
            || amax[tb] <= bmin[tb]
        {
            continue;
        }

        if axis == 1 {
            // Vertical: shape like the block solver - snap to the top surface
            // when feet cross it while landing, or the underside when the head
            // crosses it while jumping. This makes extras standable without
            // the point-overlap bounce.
            if delta < 0.0 {
                let feet = p[1] - he[1];
                let prev_feet = feet - delta;
                let top = bmax[1];
                if feet <= top + 0.001 && prev_feet >= top - 0.001 {
                    p[1] = top + he[1];
                    collided = true;
                }
            } else {
                let head = p[1] + he[1];
                let prev_head = head - delta;
                let bottom = bmin[1];
                if bottom >= prev_head - 0.001 && head >= bottom - 0.001 {
                    p[1] = bottom - he[1];
                    collided = true;
                }
            }
        } else {
            // Horizontal: only push out along this axis when the leading edge
            // crossed into the box's face this step, so a player flush against
            // a box isn't teleported through it while sliding along.
            let (leading, near_face) = if delta > 0.0 {
                (start_axis + he[axis], bmin[axis])
            } else {
                (start_axis - he[axis], bmax[axis])
            };
            let ahead = if delta > 0.0 {
                bmin[axis] >= leading
            } else {
                bmax[axis] <= leading
            };
            if !ahead {
                continue;
            }
            let stop = if delta > 0.0 {
                near_face - he[axis]
            } else {
                near_face + he[axis]
            };
            let closer = if delta > 0.0 {
                stop < p[axis]
            } else {
                stop > p[axis]
            };
            if closer {
                p[axis] = stop;
            }
            collided = true;
        }
    }

    *pos = Vec3::from_array(p);
    collided
}

pub struct MoveResult {
    pub pos: Vec3,
    pub hit_x: bool,
    pub hit_y: bool,
    pub hit_z: bool,
    pub on_ground: bool,
    /// Unit normal of the supporting surface when grounded; else +Y.
    pub floor_normal: Vec3,
    /// Unit normal of the last horizontal wall hit (zero if none).
    pub wall_normal: Vec3,
}

pub fn move_and_collide(
    mut pos: Vec3,
    he: Vec3,
    delta: Vec3,
    level: &LevelDocument,
    extras: &[(Vec3, Vec3)],
) -> MoveResult {
    let mut wall_normal = Vec3::ZERO;
    let hit_x = resolve_axis(&mut pos, he, delta.x, 0, level, extras);
    if hit_x {
        wall_normal = if delta.x > 0.0 { Vec3::NEG_X } else { Vec3::X };
    }
    let hit_z = resolve_axis(&mut pos, he, delta.z, 2, level, extras);
    if hit_z {
        wall_normal = if delta.z > 0.0 { Vec3::NEG_Z } else { Vec3::Z };
    }
    let hit_y = resolve_axis(&mut pos, he, delta.y, 1, level, extras);
    let on_ground = hit_y && delta.y < 0.0;
    let floor_normal = if on_ground {
        floor_normal_at(level, pos.x, pos.z)
    } else {
        Vec3::Y
    };
    MoveResult {
        pos,
        hit_x,
        hit_y,
        hit_z,
        on_ground,
        floor_normal,
        wall_normal,
    }
}

/// Formal floor normal from a small finite-difference of surface height.
pub fn floor_normal_at(level: &LevelDocument, wx: f32, wz: f32) -> Vec3 {
    const EPS: f32 = 0.25;
    let h = ground_height(level, wx, wz);
    if !h.is_finite() {
        return Vec3::Y;
    }
    let hx_p = ground_height(level, wx + EPS, wz);
    let hx_m = ground_height(level, wx - EPS, wz);
    let hz_p = ground_height(level, wx, wz + EPS);
    let hz_m = ground_height(level, wx, wz - EPS);

    let dx = match (hx_p.is_finite(), hx_m.is_finite()) {
        (true, true) => (hx_p - hx_m) / (2.0 * EPS),
        (true, false) => (hx_p - h) / EPS,
        (false, true) => (h - hx_m) / EPS,
        _ => 0.0,
    };
    let dz = match (hz_p.is_finite(), hz_m.is_finite()) {
        (true, true) => (hz_p - hz_m) / (2.0 * EPS),
        (true, false) => (hz_p - h) / EPS,
        (false, true) => (h - hz_m) / EPS,
        _ => 0.0,
    };

    let n = Vec3::new(-dx, 1.0, -dz);
    let len = n.length();
    if len > 1e-5 { n / len } else { Vec3::Y }
}

fn sphere_aabb_hit(center: Vec3, radius: f32, bmin: Vec3, bmax: Vec3) -> bool {
    let q = center.clamp(bmin, bmax);
    center.distance_squared(q) <= radius * radius
}

fn camera_solid_aabb(level: &LevelDocument, cell: IVec3) -> (Vec3, Vec3) {
    let bmin = cell.as_vec3();
    let bmax = bmin + Vec3::ONE;
    let Some(b) = level.get_block(cell) else {
        return (bmin, bmax);
    };
    let (lo, hi) = match b.shape {
        BlockShape::Full => (0.0, 1.0),
        BlockShape::Half => (0.0, 0.5),
        BlockShape::TopHalf => (0.5, 1.0),
        BlockShape::Thin => (1.0 - rustbox_format::block::THIN_HEIGHT, 1.0),
        _ => (0.0, 1.0),
    };
    (
        Vec3::new(bmin.x, bmin.y + lo, bmin.z),
        Vec3::new(bmax.x, bmin.y + hi, bmax.z),
    )
}

/// True if a sphere overlaps any solid cell or extra box.
pub fn sphere_hits_world(
    level: &LevelDocument,
    center: Vec3,
    radius: f32,
    extras: &[(Vec3, Vec3)],
) -> bool {
    let min = center - Vec3::splat(radius);
    let max = center + Vec3::splat(radius);
    for x in (min.x.floor() as i32)..=(max.x.floor() as i32) {
        for y in (min.y.floor() as i32)..=(max.y.floor() as i32) {
            for z in (min.z.floor() as i32)..=(max.z.floor() as i32) {
                let cell = IVec3::new(x, y, z);
                if !level.is_solid(cell) {
                    continue;
                }
                let (bmin, bmax) = camera_solid_aabb(level, cell);
                if sphere_aabb_hit(center, radius, bmin, bmax) {
                    return true;
                }
            }
        }
    }
    for (ec, ehe) in extras {
        if sphere_aabb_hit(center, radius, *ec - *ehe, *ec + *ehe) {
            return true;
        }
    }
    false
}

/// March a sphere from `origin` along `dir` up to `max_dist`. Returns free
/// distance.
pub fn sphere_cast_distance(
    level: &LevelDocument,
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
    radius: f32,
    extras: &[(Vec3, Vec3)],
) -> f32 {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO || max_dist <= 0.0 {
        return 0.0;
    }
    let mut t = 0.0;
    let step = (radius * 0.4).clamp(0.08, 0.25);
    let mut last_free = 0.0;
    while t <= max_dist {
        let p = origin + dir * t;
        if sphere_hits_world(level, p, radius, extras) {
            let mut lo = last_free;
            let mut hi = t;
            for _ in 0..6 {
                let mid = (lo + hi) * 0.5;
                if sphere_hits_world(level, origin + dir * mid, radius, extras) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            return lo;
        }
        last_free = t;
        t += step;
    }
    max_dist
}

/// Pull a desired camera eye in along the focus->eye segment so the sphere
/// stays free. If a wall sits immediately behind the player, the eye would
/// otherwise be forced down to ~0.6 away and zoom right into the player's
/// back - very annoying - so the camera keeps a comfortable distance instead.
const MIN_EYE_KEEP: f32 = 1.6;

pub fn collide_camera_eye(
    level: &LevelDocument,
    focus: Vec3,
    desired_eye: Vec3,
    radius: f32,
    skin: f32,
    extras: &[(Vec3, Vec3)],
) -> Vec3 {
    let offset = desired_eye - focus;
    let dist = offset.length();
    if dist < 1e-4 {
        return desired_eye;
    }
    let dir = offset / dist;
    // Start the sweep from just behind the focus along the view line. The old
    // `focus + Vec3::Y * 0.35` origin sat above the player's head, so a low
    // overhang / ceiling right above the head blocked the whole cast and the
    // eye snapped to the `min` clamp; it also left the origin sphere inside
    // side walls the player is flush against.
    let origin = focus - dir * (radius + skin);
    let free = sphere_cast_distance(level, origin, dir, dist, radius, extras);
    // When a wall is this close behind the player, pulling the eye in to find
    // a free spot only zooms the camera into the player. Disable that: hold the
    // normal zoom distance and let the wall clip instead of the camera.
    if free <= radius + skin + MIN_EYE_KEEP {
        return focus + dir * dist;
    }
    // Otherwise keep the eye out in front of the closest wall, but never
    // closer than MIN_EYE_KEEP so the camera doesn't swoop into the rig.
    let use_dist = (free - skin).min(dist).max(MIN_EYE_KEEP);
    focus + dir * use_dist
}

/// World-space height of the topmost solid surface at horizontal point
/// `(wx, wz)`. Returns `f32::NEG_INFINITY` over open void (e.g. a pit).
pub fn ground_height(level: &LevelDocument, wx: f32, wz: f32) -> f32 {
    let cx = wx.floor() as i32;
    let cz = wz.floor() as i32;
    let from = wx.max(wz).max(0.0) as i32 + 8;
    for y in ((-512 + 8)..=from).rev() {
        let cell = IVec3::new(cx, y, cz);
        let block = level.get_block(cell);
        let solid =
            block.is_some_and(|b| level.kind_is_solid(b.kind)) || level.boundary_solid(cell);
        if solid {
            return match block {
                Some(b) if level.kind_is_solid(b.kind) => surface_top_height(b, wx, wz),
                _ => y as f32 + 1.0,
            };
        }
    }
    f32::NEG_INFINITY
}

pub fn stand_headroom(
    level: &LevelDocument,
    pos: Vec3,
    he: Vec3,
    crouch_factor: f32,
    extras: &[(Vec3, Vec3)],
) -> bool {
    let lo_y = pos.y + he.y * crouch_factor;
    let hi_y = pos.y + he.y;
    let min = Vec3::new(pos.x - he.x, lo_y, pos.z - he.z);
    let max = Vec3::new(pos.x + he.x, hi_y, pos.z + he.z);
    for x in (min.x.floor() as i32)..=(max.x.floor() as i32) {
        for y in (min.y.floor() as i32)..=(max.y.floor() as i32) {
            for z in (min.z.floor() as i32)..=(max.z.floor() as i32) {
                let cell = IVec3::new(x, y, z);
                if !level.is_solid(cell) {
                    continue;
                }
                let cmin = cell.as_vec3();
                let cmax = cmin + Vec3::ONE;
                if min.x < cmax.x
                    && max.x > cmin.x
                    && min.y < cmax.y
                    && max.y > cmin.y
                    && min.z < cmax.z
                    && max.z > cmin.z
                {
                    return false;
                }
            }
        }
    }
    for (ec, ehe) in extras {
        if min.x < ec.x + ehe.x
            && max.x > ec.x - ehe.x
            && min.y < ec.y + ehe.y
            && max.y > ec.y - ehe.y
            && min.z < ec.z + ehe.z
            && max.z > ec.z - ehe.z
        {
            return false;
        }
    }
    true
}

/// The lowest horizontal midpoint-provision surface already fitted under
/// the center of the block.
pub fn slope_slide(level: &LevelDocument, center: Vec3, he: Vec3) -> Option<Vec2> {
    const SLIDE_MIN: f32 = 0.22;
    let hc = ground_height(level, center.x, center.z);
    if !hc.is_finite() {
        return None;
    }
    let dirs = [
        Vec2::X,
        Vec2::NEG_X,
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, -1.0),
    ];
    let dists = [he.x, he.x, he.z, he.z];
    let mut best: Option<(Vec2, f32)> = None;
    for (d, dist) in dirs.iter().zip(dists.iter()) {
        let h = ground_height(level, center.x + d.x * dist, center.z + d.y * dist);
        let drop = hc - h;
        if drop.is_finite() && drop >= SLIDE_MIN && best.is_none_or(|(_, bd)| drop > bd) {
            best = Some((*d, drop));
        }
    }
    best.map(|(d, _)| d)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedgeGrip {
    /// Unit horizontal direction from the player toward the wall (the grabbed face).
    pub face: Vec2,
    /// World height of the ledge lip (top surface) the player hangs on.
    pub wall_top: f32,
    /// Validated hang center (body just below the lip, clear of solids).
    pub hang_pos: Vec3,
    /// Validated pull-up center (feet on top of the lip, body clear).
    pub mantle_pos: Vec3,
}

const GRAB_MIN_ABOVE_FEET: f32 = 0.85;
const GRAB_MAX_ABOVE_FEET: f32 = 1.65;
/// How far past the capsule we sample for the wall face.
const WALL_PROBE: f32 = 0.08;
/// How far below the lip the hang center rests (after the body half-height).
const HANG_DROP: f32 = 0.14;
/// How far onto the block the mantle pose sits.
const MANTLE_INSET: f32 = 0.35;

fn is_grabbable_shape(shape: BlockShape) -> bool {
    matches!(
        shape,
        BlockShape::Full | BlockShape::Half | BlockShape::TopHalf | BlockShape::VerticalSlab
    )
}

fn is_grabbable_lip(level: &LevelDocument, cell: IVec3) -> bool {
    if !level.is_solid(cell) || level.is_solid(cell + IVec3::Y) {
        return false;
    }
    match level.get_block(cell) {
        Some(b) => is_grabbable_shape(b.shape),
        // Boundary solids (walls/floor) are always boxy.
        None => true,
    }
}

/// Does the AABB at `center` overlap any solid cell?
pub fn aabb_hits_solid(level: &LevelDocument, center: Vec3, he: Vec3) -> bool {
    let min = center - he + Vec3::splat(0.02);
    let max = center + he - Vec3::splat(0.02);
    for x in (min.x.floor() as i32)..=(max.x.floor() as i32) {
        for y in (min.y.floor() as i32)..=(max.y.floor() as i32) {
            for z in (min.z.floor() as i32)..=(max.z.floor() as i32) {
                if level.is_solid(IVec3::new(x, y, z)) {
                    return true;
                }
            }
        }
    }
    false
}

/// Nudge `p` away from the wall along `face` until it no longer overlaps solid.
fn push_out_of_wall(level: &LevelDocument, mut p: Vec3, he: Vec3, face: Vec2) -> Vec3 {
    for _ in 0..4 {
        if !aabb_hits_solid(level, p, he) {
            break;
        }
        p.x -= face.x * 0.05;
        p.z -= face.y * 0.05;
    }
    p
}

fn face_adjacent_solid(level: &LevelDocument, center: Vec3, face: Vec2, he: Vec3) -> bool {
    let reach_x = he.x.max(he.z);
    let px = center.x + face.x * (reach_x + 0.01);
    let pz = center.z + face.y * (reach_x + 0.01);
    let c = IVec3::new(
        px.floor() as i32,
        center.y.floor() as i32,
        pz.floor() as i32,
    );
    let y0 = (center.y - 0.2).floor() as i32;
    let y1 = (center.y + 0.4).floor() as i32;
    (y0..=y1).any(|y| level.is_solid(IVec3::new(c.x, y, c.z)))
}

/// When falling into a solid wall whose lip is within the player's reach,
/// return a place to grab so the fall stops and a grab/climb becomes possible.
pub fn ledge_grip(
    level: &LevelDocument,
    center: Vec3,
    he: Vec3,
    vel: Vec3,
    hit_x: bool,
    hit_z: bool,
) -> Option<LedgeGrip> {
    let feet = center.y - he.y;
    let approach = Vec2::new(vel.x, vel.z);
    let dirs: [(Vec2, f32, bool); 4] = [
        (Vec2::X, he.x, hit_x),
        (Vec2::NEG_X, he.x, hit_x),
        (Vec2::new(0.0, 1.0), he.z, hit_z),
        (Vec2::new(0.0, -1.0), he.z, hit_z),
    ];

    for (face, radius, hit_axis) in dirs {
        // Was this exact face hit this frame while moving into it?
        let hit_into = (face.x > 0.5 && hit_axis && vel.x >= 0.0)
            || (face.x < -0.5 && hit_axis && vel.x <= 0.0)
            || (face.y > 0.5 && hit_axis && vel.z >= 0.0)
            || (face.y < -0.5 && hit_axis && vel.z <= 0.0);
        let moving_in = approach.dot(face) > 0.15;
        let pressed =
            approach.length_squared() < 0.01 && face_adjacent_solid(level, center, face, he);
        if !(hit_into || moving_in || pressed) {
            continue;
        }

        let probe = Vec3::new(
            center.x + face.x * (radius + WALL_PROBE),
            center.y,
            center.z + face.y * (radius + WALL_PROBE),
        );
        let wt = ground_height(level, probe.x, probe.z);
        if !wt.is_finite() {
            continue;
        }
        let above_feet = wt - feet;
        if above_feet < GRAB_MIN_ABOVE_FEET || above_feet > GRAB_MAX_ABOVE_FEET {
            continue;
        }
        let lip_cell = IVec3::new(
            probe.x.floor() as i32,
            (wt - 0.01).floor() as i32,
            probe.z.floor() as i32,
        );
        if !is_grabbable_lip(level, lip_cell) {
            continue;
        }

        // Hang: just below the lip, clear of the wall.
        let hang = Vec3::new(center.x, wt - he.y + HANG_DROP, center.z);
        let hang = push_out_of_wall(level, hang, he, face);
        if aabb_hits_solid(level, hang, he) {
            continue;
        }

        // Mantle: standing on top of the block, slightly inset across the face.
        let mantle = Vec3::new(
            lip_cell.x as f32 + 0.5 - face.x * MANTLE_INSET,
            wt + he.y + 0.01,
            lip_cell.z as f32 + 0.5 - face.y * MANTLE_INSET,
        );
        if aabb_hits_solid(level, mantle, he) {
            continue;
        }

        return Some(LedgeGrip {
            face,
            wall_top: wt,
            hang_pos: hang,
            mantle_pos: mantle,
        });
    }

    None
}

pub fn overlaps_kind(center: Vec3, he: Vec3, level: &LevelDocument, kind: BlockKind) -> bool {
    let he = he + Vec3::splat(0.06);
    let min = center - he;
    let max = center + he;
    for x in (min.x.floor() as i32)..=(max.x.floor() as i32) {
        for y in (min.y.floor() as i32)..=(max.y.floor() as i32) {
            for z in (min.z.floor() as i32)..=(max.z.floor() as i32) {
                if level.get_kind(IVec3::new(x, y, z)) == Some(kind) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn raycast_present(
    level: &LevelDocument,
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
) -> Option<(IVec3, IVec3)> {
    let dir = dir.normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }

    let mut cell = IVec3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    if level.is_solid(cell) {
        return Some((cell, IVec3::ZERO));
    }

    let step = IVec3::new(
        dir.x.signum() as i32,
        dir.y.signum() as i32,
        dir.z.signum() as i32,
    );

    let boundary = |o: f32, d: f32, c: i32| -> f32 {
        if d > 0.0 {
            (c as f32 + 1.0 - o) / d
        } else if d < 0.0 {
            (c as f32 - o) / d
        } else {
            f32::INFINITY
        }
    };

    let mut t_max = Vec3::new(
        boundary(origin.x, dir.x, cell.x),
        boundary(origin.y, dir.y, cell.y),
        boundary(origin.z, dir.z, cell.z),
    );
    let t_delta = Vec3::new(
        if dir.x != 0.0 {
            (1.0 / dir.x).abs()
        } else {
            f32::INFINITY
        },
        if dir.y != 0.0 {
            (1.0 / dir.y).abs()
        } else {
            f32::INFINITY
        },
        if dir.z != 0.0 {
            (1.0 / dir.z).abs()
        } else {
            f32::INFINITY
        },
    );

    let mut t = 0.0;
    while t <= max_dist {
        let normal;
        if t_max.x < t_max.y && t_max.x < t_max.z {
            cell.x += step.x;
            t = t_max.x;
            t_max.x += t_delta.x;
            normal = IVec3::new(-step.x, 0, 0);
        } else if t_max.y < t_max.z {
            cell.y += step.y;
            t = t_max.y;
            t_max.y += t_delta.y;
            normal = IVec3::new(0, -step.y, 0);
        } else {
            cell.z += step.z;
            t = t_max.z;
            t_max.z += t_delta.z;
            normal = IVec3::new(0, 0, -step.z);
        }

        if level.is_solid(cell) {
            return Some((cell, normal));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const HE: Vec3 = Vec3::new(0.3, 0.9, 0.3);

    fn wall_level() -> LevelDocument {
        LevelDocument::default()
    }

    fn standing(x: f32, z: f32) -> Vec3 {
        Vec3::new(x, 1.9, z)
    }

    #[test]
    fn run_along_wall_advances_without_snapping() {
        let level = wall_level();
        // Flush against the +Z boundary wall (z=15) at z=14.7, running +X.
        let r = move_and_collide(
            standing(10.0, 14.7),
            HE,
            Vec3::new(0.13, 0.0, 0.0),
            &level,
            &[],
        );
        // Must advance by the delta, not jump to a cell face or backward.
        assert!((r.pos.x - 10.13).abs() < 0.01, "x snapped to {}", r.pos.x);
        assert!((r.pos.z - 14.7).abs() < 0.01, "z drifted to {}", r.pos.z);
        assert!(!r.hit_x, "unexpected hit_x");
    }

    #[test]
    fn run_into_wall_stops_at_face() {
        let level = wall_level();
        // Running +X into the +X wall (face at x=15).
        let r = move_and_collide(
            standing(11.0, 0.0),
            HE,
            Vec3::new(4.0, 0.0, 0.0),
            &level,
            &[],
        );
        // +X face must stop flush against the wall.
        assert!(
            (r.pos.x + HE.x - 15.0).abs() < 0.01,
            "ended at x={}",
            r.pos.x
        );
        assert!(r.hit_x);
    }

    #[test]
    fn run_along_wall_then_into_corner_stops_at_face() {
        let level = wall_level();
        // Flush against +Z wall, deep +X move toward the +X/+Z corner.
        let r = move_and_collide(
            standing(10.0, 14.7),
            HE,
            Vec3::new(6.0, 0.0, 0.0),
            &level,
            &[],
        );
        // Must stop at the +X wall face, staying flush with the +Z wall.
        assert!(
            (r.pos.x + HE.x - 15.0).abs() < 0.01,
            "snapped to x={}",
            r.pos.x
        );
        assert!((r.pos.z - 14.7).abs() < 0.01, "z snapped to {}", r.pos.z);
        assert!(r.hit_x);
    }

    #[test]
    fn diagonal_into_corner_stays_inside() {
        let level = wall_level();
        let r = move_and_collide(
            standing(10.0, 10.0),
            HE,
            Vec3::new(6.0, 0.0, 6.0),
            &level,
            &[],
        );
        // Must stop at both walls, not overshoot into or around the corner.
        assert!(
            (r.pos.x + HE.x - 15.0).abs() < 0.01,
            "x overshot to {}",
            r.pos.x
        );
        assert!(
            (r.pos.z + HE.z - 15.0).abs() < 0.01,
            "z overshot to {}",
            r.pos.z
        );
        assert!(r.hit_x && r.hit_z);
    }

    #[test]
    fn free_flat_ground_advance() {
        let level = wall_level();
        // Far from the starter blocks and walls: steps should accumulate smoothly.
        let mut pos = standing(0.0, 10.0);
        let dt = Vec3::new(0.117, 0.0, 0.0);
        for _ in 0..60 {
            let r = move_and_collide(pos, HE, dt, &level, &[]);
            pos = r.pos;
            assert!(!r.hit_x, "blocked at x={}", pos.x);
        }
        // 60 * 0.117 = 7.02, give room for float error.
        assert!((pos.x - 7.02).abs() < 0.05, "advanced to {}", pos.x);
    }

    #[test]
    fn jump_flush_against_wall_rises_not_snaps_down() {
        let level = wall_level();
        // Flush against the +X boundary wall (face at x=15), jump straight up.
        let r = move_and_collide(
            standing(14.7, 0.0),
            HE,
            Vec3::new(0.0, 0.5, 0.0),
            &level,
            &[],
        );
        // The wall's side cells must not teleport the player down to its bottom.
        assert!((r.pos.y - 2.4).abs() < 0.01, "y snapped to {}", r.pos.y);
        assert!(!r.hit_y, "false vertical hit near wall");
    }

    #[test]
    fn jump_flush_against_placed_wall_rises() {
        let mut level = wall_level();
        // A 3-tall stone wall at x=5 (cells y=1..=3, bottom at y=1).
        for y in 1..=3 {
            level.set_block(
                IVec3::new(5, y, 0),
                Some(BlockData::new([5, y, 0], BlockKind::Stone)),
            );
        }
        // Flush against the wall at x=4.7, jump straight up.
        let r = move_and_collide(
            standing(4.7, 0.0),
            HE,
            Vec3::new(0.0, 0.5, 0.0),
            &level,
            &[],
        );
        assert!((r.pos.y - 2.4).abs() < 0.01, "y snapped to {}", r.pos.y);
        assert!(!r.hit_y, "false vertical hit near wall");
    }

    #[test]
    fn jump_into_ceiling_stops_under_it() {
        // Ceiling block at y=3 (bottom at 3.0) directly overhead.
        let mut level = wall_level();
        level.set_block(
            IVec3::new(0, 3, 0),
            Some(BlockData::new([0, 3, 0], BlockKind::Stone)),
        );
        // Standing at x=0 (block spans cells -1..1), jump hard: must stop at
        // the block's underside, feet just below y=3.
        let r = move_and_collide(
            standing(0.0, 0.0),
            HE,
            Vec3::new(0.0, 2.0, 0.0),
            &level,
            &[],
        );
        assert!(
            (r.pos.y + HE.y - 3.0).abs() < 0.01,
            "ended at y={}",
            r.pos.y
        );
        assert!(r.hit_y);
    }

    #[test]
    fn crouched_walk_along_wall_advances() {
        let level = wall_level();
        // Crouched half-extents, flush against the +Z wall at z=14.7, run +X.
        let he_c = Vec3::new(0.3, HE.y * 0.55, 0.3);
        let r = move_and_collide(
            Vec3::new(10.0, 1.0 + he_c.y, 14.7),
            he_c,
            Vec3::new(0.13, 0.0, 0.0),
            &level,
            &[],
        );
        assert!((r.pos.x - 10.13).abs() < 0.01, "x snapped to {}", r.pos.x);
        assert!((r.pos.z - 14.7).abs() < 0.01, "z snapped to {}", r.pos.z);
    }

    #[test]
    fn crouched_jump_flush_against_wall_rises() {
        let level = wall_level();
        // Crouched and jumping up while flush against the +X boundary wall.
        let he_c = Vec3::new(0.3, HE.y * 0.55, 0.3);
        let r = move_and_collide(
            Vec3::new(14.7, 1.0 + he_c.y, 0.0),
            he_c,
            Vec3::new(0.0, 0.5, 0.0),
            &level,
            &[],
        );
        let expected = 1.0 + he_c.y + 0.5;
        assert!(
            (r.pos.y - expected).abs() < 0.01,
            "y snapped to {}",
            r.pos.y
        );
        assert!(!r.hit_y, "false vertical hit near wall");
    }

    #[test]
    fn crouched_diagonal_into_corner_stays_inside() {
        let level = wall_level();
        let he_c = Vec3::new(0.3, HE.y * 0.55, 0.3);
        let r = move_and_collide(
            Vec3::new(10.0, 1.0 + he_c.y, 10.0),
            he_c,
            Vec3::new(6.0, 0.0, 6.0),
            &level,
            &[],
        );
        assert!(
            (r.pos.x + he_c.x - 15.0).abs() < 0.01,
            "x overshot to {}",
            r.pos.x
        );
        assert!(
            (r.pos.z + he_c.z - 15.0).abs() < 0.01,
            "z overshot to {}",
            r.pos.z
        );
        assert!(r.hit_x && r.hit_z);
    }

    #[test]
    fn stand_headroom_blocks_low_ceiling() {
        // Ceiling block at y=2 (bottom at 2.0). Crouched under it at y=1.495
        // (head 1.99) there's room; standing (head 2.395) would clip.
        let mut level = wall_level();
        level.set_block(
            IVec3::new(0, 2, 0),
            Some(BlockData::new([0, 2, 0], BlockKind::Stone)),
        );
        let crouched_y = 1.0 + HE.y * 0.55;
        let under = Vec3::new(0.0, crouched_y, 0.0);
        assert!(
            !stand_headroom(&level, under, HE, 0.55, &[]),
            "cannot stand under ceiling"
        );
        // Sliding slightly past the overhang edge (x beyond cell 0) frees up.
        let past = Vec3::new(1.6, crouched_y, 0.0);
        assert!(stand_headroom(&level, past, HE, 0.55, &[]), "past overhang");
    }

    #[test]
    fn stand_headroom_open_floor_and_beside_wall() {
        let level = wall_level();
        let crouched_y = 1.0 + HE.y * 0.55;
        // Open floor: always headroom.
        assert!(stand_headroom(
            &level,
            Vec3::new(0.0, crouched_y, 10.0),
            HE,
            0.55,
            &[]
        ));
        // Flush against the +X boundary wall: the wall is beside, not above.
        assert!(stand_headroom(
            &level,
            Vec3::new(14.7, crouched_y, 0.0),
            HE,
            0.55,
            &[]
        ));
    }

    /// A closed relay gate is a box centered at (4, 2.0, 0) with half-extents
    /// (0.5, 1.0, 0.2): x in [3.5, 4.5], y in [1.0, 3.0], z in [-0.2, 0.2].
    fn gate_extra() -> (Vec3, Vec3) {
        (Vec3::new(4.0, 2.0, 0.0), Vec3::new(0.5, 1.0, 0.2))
    }

    #[test]
    fn walk_into_gate_stops_at_face_no_bounce() {
        let level = wall_level();
        // Player approaching from the left; right edge at 3.4, just short of
        // the gate face at x=3.5. Body spans y [1.0, 2.8], overlapping the
        // gate's vertical extent [1.0, 3.0]. z=0 overlaps the gate's thin z.
        let r = move_and_collide(
            Vec3::new(3.1, 1.9, 0.0),
            HE,
            Vec3::new(0.3, 0.0, 0.0),
            &level,
            &[gate_extra()],
        );
        // Must stop flush against the gate face (right edge at 3.5), not sink
        // into or teleport past it.
        assert!(
            (r.pos.x + HE.x - 3.5).abs() < 0.05,
            "player moved to x={} (right edge {})",
            r.pos.x,
            r.pos.x + HE.x
        );
        assert!(r.hit_x, "expected an X collision");
    }

    #[test]
    fn land_on_top_of_gate_is_stable() {
        let level = wall_level();
        // Feet start just above the gate top (y=3.0) and move down 0.5.
        let start = Vec3::new(4.0, 3.2 + HE.y, 0.0);
        let r = move_and_collide(
            start,
            HE,
            Vec3::new(0.0, -0.5, 0.0),
            &level,
            &[gate_extra()],
        );
        // Feet must come to rest exactly on the gate top (y=3.0), ground flag set.
        assert!(
            (r.pos.y - HE.y - 3.0).abs() < 0.05,
            "feet at {}",
            r.pos.y - HE.y
        );
        assert!(r.hit_y && r.on_ground, "should land and be grounded");

        // A second downward step must not fall through - stays grounded.
        let r2 = move_and_collide(
            Vec3::new(r.pos.x, r.pos.y, r.pos.z),
            HE,
            Vec3::new(0.0, -0.2, 0.0),
            &level,
            &[gate_extra()],
        );
        assert!(
            (r2.pos.y - HE.y - 3.0).abs() < 0.05,
            "no bounce-through: feet at {}",
            r2.pos.y - HE.y
        );
    }
}
