use bevy::prelude::*;

use super::block::BlockKind;
use super::level::LevelDocument;

pub fn is_solid(level: &LevelDocument, cell: IVec3) -> bool {
    level.get_block(cell).map(|k| k.is_solid()).unwrap_or(false)
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
                if !is_solid(level, IVec3::new(x, y, z)) {
                    continue;
                }
                let cell = [x, y, z];
                if delta > 0.0 {
                    p[axis] = cell[axis] as f32 - he[axis];
                } else {
                    p[axis] = cell[axis] as f32 + 1.0 + he[axis];
                }
                collided = true;
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
                if level.get_block(IVec3::new(x, y, z)) == Some(kind) {
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

    if level.get_block(cell).is_some() {
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

        if level.get_block(cell).is_some() {
            return Some((cell, normal));
        }
    }
    None
}
