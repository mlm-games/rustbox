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
    }
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
                let solid = block.is_some_and(|b| b.kind.is_solid()) || boundary;
                if !solid {
                    continue;
                }

                if axis == 1 {
                    // Vertical movement is special-cased so ramps/slabs give a
                    // shaped top surface instead of a flat cell top.
                    if delta < 0.0 {
                        let top = match block {
                            Some(b) if b.kind.is_solid() => surface_top_height(b, v.x, v.z),
                            _ => cell.y as f32 + 1.0,
                        };
                        // Only snap when the column under the player is solid
                        // (skip the empty halves of partial shapes) and the
                        // player's bottom crosses the surface from above this
                        // frame (`prev_feet` = feet before the move).
                        let column_solid = match block {
                            Some(b) if b.kind.is_solid() => {
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
                        p[1] = cell.y as f32 - he[1];
                        collided = true;
                    }
                } else {
                    // Horizontal: only block if the player's vertical span
                    // overlaps the solid material on that face (slabs and
                    // slopes leave gaps a small player can slip under).
                    let (flo, fhi) = match block {
                        Some(b) if b.kind.is_solid() => {
                            face_solid_range(b.shape, b.rot, axis, if delta > 0.0 { 0 } else { 1 })
                        }
                        _ => (0.0, 1.0),
                    };
                    let vlo = p[1] - he[1];
                    let vhi = p[1] + he[1];
                    if !vertical_overlap(vlo, vhi, cell.y as f32 + flo, cell.y as f32 + fhi) {
                        continue;
                    }
                    if delta > 0.0 {
                        p[axis] = cell[axis] as f32 - he[axis];
                    } else {
                        p[axis] = cell[axis] as f32 + 1.0 + he[axis];
                    }
                    collided = true;
                }
            }
        }
    }

    for (ec, ehe) in extras {
        let ehe = ehe.to_array();
        let ec = ec.to_array();
        let amin = p.map(|v| v - 0.001);
        let amax = p.map(|v| v + 0.001);
        if amin[0] < ec[0] + ehe[0]
            && amax[0] > ec[0] - ehe[0]
            && amin[1] < ec[1] + ehe[1]
            && amax[1] > ec[1] - ehe[1]
            && amin[2] < ec[2] + ehe[2]
            && amax[2] > ec[2] - ehe[2]
        {
            if delta > 0.0 {
                p[axis] = ec[axis] - ehe[axis] - he[axis];
            } else {
                p[axis] = ec[axis] + ehe[axis] + he[axis];
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
}

pub fn move_and_collide(
    mut pos: Vec3,
    he: Vec3,
    delta: Vec3,
    level: &LevelDocument,
    extras: &[(Vec3, Vec3)],
) -> MoveResult {
    let hit_x = resolve_axis(&mut pos, he, delta.x, 0, level, extras);
    let hit_z = resolve_axis(&mut pos, he, delta.z, 2, level, extras);
    let hit_y = resolve_axis(&mut pos, he, delta.y, 1, level, extras);
    let on_ground = hit_y && delta.y < 0.0;
    MoveResult {
        pos,
        hit_x,
        hit_y,
        hit_z,
        on_ground,
    }
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
