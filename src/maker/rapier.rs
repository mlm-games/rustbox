use bevy_ecs::prelude::{Entity, With, Without, World};
use glam::{Quat, Vec3};
use repame_rapier3d::{BodyKind, BodySnapshot3d, RapierBody3d, RapierWorld3d, VelocityRequest3d};

use super::camera::CameraRig;
use super::collision::aabb_hits_solid;
use super::entities_runtime::{Held, LevelEnt, Throwable, Transform};
use super::entity_data::LevelEntityId;
use super::interaction::solid_blocks;
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::{PlayIntent, Player, PlayerTransform};
use super::props::{RuntimeSolids, Velocity};

/// Half the 0.8 m crate cube: matches `Collider::cuboid(0.4, 0.4, 0.4)`.
const CRATE_HALF: f32 = 0.4;

/// `spawn_bodies` applies this as *additional* mass on top of the collider's
/// own density-derived mass (0.512 kg for a 0.8 m cube at density 1.0), so 0
/// keeps the bevy_rapier-equivalent total of 0.512 kg.
const CRATE_MASS: f32 = 0.0;

const THROW_SPEED: f32 = 14.0;
const THROW_LIFT: f32 = 3.5;

fn forward_of(yaw: f32) -> Vec3 {
    let (sin, cos) = yaw.sin_cos();
    Vec3::new(-sin, 0.0, -cos)
}

pub(super) fn crate_body(pos: Vec3, rot: Quat) -> RapierBody3d {
    RapierBody3d {
        kind: BodyKind::Dynamic,
        spawn_pos: pos,
        spawn_rot: rot,
        half_extents: Vec3::splat(CRATE_HALF),
        mass: CRATE_MASS,
        ..Default::default()
    }
}

/// Turn an entity back into a free dynamic crate: the body is (re)spawned at
/// its current transform on the next tick, so no stale velocity survives.
/// `linear` is the throw velocity, applied as a set (mass independent) while
/// the fresh body starts at rest anyway.
pub(super) fn make_dynamic(world: &mut World, entity: Entity, linear: Option<Vec3>) {
    let (pos, rot) = match world.get::<Transform>(entity) {
        Some(tf) => (tf.translation, tf.rotation),
        None => return,
    };
    if world.get::<BodySnapshot3d>(entity).is_none() {
        world.entity_mut(entity).insert(BodySnapshot3d::default());
    }
    world.entity_mut(entity).remove::<RapierBody3d>();
    world.entity_mut(entity).insert(crate_body(pos, rot));
    match linear {
        Some(linear) => {
            world.entity_mut(entity).insert(VelocityRequest3d {
                linear,
                angular: Vec3::ZERO,
            });
            world.entity_mut(entity).insert(Velocity {
                linear,
                angular: Vec3::ZERO,
            });
        }
        None => {
            world
                .entity_mut(entity)
                .insert(VelocityRequest3d::default());
            world.entity_mut(entity).insert(Velocity::zero());
        }
    }
    world.entity_mut(entity).remove::<Held>();
}

/// Drop any held crates when leaving Play: the kinematic body would otherwise
/// float mid-air in Edit (or be carried into the next run's spawn).
pub fn release_held_on_mode_change(world: &mut World) {
    if *world.resource::<MakerMode>() == MakerMode::Play {
        return;
    }
    let held: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, (With<Throwable>, With<Held>)>();
        q.iter(world).collect()
    };
    for e in held {
        make_dynamic(world, e, None);
    }
}

/// While a crate is held it is parked in front of the player, tracking the
/// camera heading. The target is validated against level + runtime solids: a
/// blocked target keeps the old position instead of teleporting through walls.
/// Held crates carry no rigid body, so Rapier cannot shove the player.
pub fn move_held_objects(world: &mut World) {
    let (yaw, player_pos) = {
        let yaw = world.resource::<CameraRig>().yaw;
        let mut q = world.query_filtered::<&PlayerTransform, With<Player>>();
        let Ok(tf) = q.single(world) else {
            return;
        };
        (yaw, tf.translation)
    };
    let held: Vec<(Entity, LevelEntityId, Vec3)> = {
        let mut q = world
            .query_filtered::<(Entity, &LevelEnt, &Transform), (With<Held>, Without<Player>)>();
        q.iter(world)
            .map(|(e, le, tf)| (e, le.id, tf.translation))
            .collect()
    };
    if held.is_empty() {
        return;
    }
    let forward = forward_of(yaw);
    let target = player_pos + forward * 1.2 - Vec3::Y * 0.15;
    let mut drops = Vec::new();
    let mut holds = Vec::new();
    {
        let level = world.resource::<LevelDocument>();
        let solids = world.resource::<RuntimeSolids>();
        let he = Vec3::splat(CRATE_HALF);
        for (e, id, pos) in held {
            // Far-away hold (retry/load teleported the player): drop in place
            // instead of dragging the old crate across the level to the spawn.
            // Blocked target (wall): also drop rather than teleport through.
            let far = pos.distance(target) > 6.0;
            let blocked =
                aabb_hits_solid(level, target, he) || solid_blocks(solids, id, target, he);
            if far || blocked {
                drops.push(e);
            } else {
                holds.push(e);
            }
        }
    }
    if !holds.is_empty() {
        let mut q = world
            .query_filtered::<(&mut Transform, &mut Velocity), (With<Held>, Without<Player>)>();
        for e in holds {
            if let Ok((mut tf, mut vel)) = q.get_mut(world, e) {
                tf.translation = target;
                vel.linear = Vec3::ZERO;
                vel.angular = Vec3::ZERO;
            }
        }
    }
    for e in drops {
        make_dynamic(world, e, None);
    }
}

/// F (or Right Trigger) near a Throwable picks it up (it loses its body and
/// rides in front of the player); F while holding throws it (fresh dynamic
/// body with the throw velocity set).
pub fn pickup_throwables(world: &mut World) {
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    if !world.resource::<PlayIntent>().throw_pressed {
        return;
    }
    let (yaw, player_pos) = {
        let yaw = world.resource::<CameraRig>().yaw;
        let mut q = world.query_filtered::<&PlayerTransform, With<Player>>();
        let Ok(tf) = q.single(world) else {
            return;
        };
        (yaw, tf.translation)
    };
    let forward = forward_of(yaw);

    let held: Vec<Entity> = {
        let mut q = world.query_filtered::<Entity, (With<Throwable>, With<Held>)>();
        q.iter(world).collect()
    };
    if let Some(e) = held.first().copied() {
        make_dynamic(world, e, Some(forward * THROW_SPEED + Vec3::Y * THROW_LIFT));
        return;
    }

    // Otherwise pick up the nearest crate in front of the player within reach.
    // Requires line-of-sight: no level solid or runtime solid between player
    // chest and crate (prevents grabbing through walls).
    let crates: Vec<(Entity, LevelEntityId, Vec3)> = {
        let mut q = world
            .query_filtered::<(Entity, &LevelEnt, &Transform), (With<Throwable>, Without<Held>)>();
        q.iter(world)
            .map(|(e, le, tf)| (e, le.id, tf.translation))
            .collect()
    };
    let mut best: Option<(Entity, f32)> = None;
    {
        let level = world.resource::<LevelDocument>();
        let solids = world.resource::<RuntimeSolids>();
        for (e, id, pos) in crates {
            let to = (pos - player_pos).normalize_or_zero();
            let facing = forward.dot(to);
            let d = player_pos.distance(pos);
            if d < 1.6 && facing > 0.15 && best.map_or(true, |(_, bd)| d < bd) {
                let mid = (player_pos + pos) * 0.5;
                let blocked = aabb_hits_solid(level, mid, Vec3::splat(0.2))
                    || solid_blocks(solids, id, mid, Vec3::splat(0.2));
                if blocked {
                    continue;
                }
                best = Some((e, d));
            }
        }
    }
    if let Some((e, _)) = best {
        world.entity_mut(e).remove::<RapierBody3d>();
        world.entity_mut(e).remove::<VelocityRequest3d>();
        world.entity_mut(e).insert(Held);
        world.entity_mut(e).insert(Velocity::zero());
    }
}

/// Body state is authoritative for the Transform/Velocity of anything that
/// carries a rigid body: mirrors bevy_rapier's per-frame Transform writeback.
/// Held crates have no body, so their Transform stays game-authoritative.
pub fn write_back_bodies(world: &mut World) {
    if !world.contains_resource::<RapierWorld3d>() {
        return;
    }
    let mut synced: Vec<(Entity, BodySnapshot3d)> = {
        let mut q = world
            .query_filtered::<(Entity, &BodySnapshot3d), (With<RapierBody3d>, Without<Held>)>();
        q.iter(world).map(|(e, s)| (e, *s)).collect()
    };
    {
        let rapier = world.resource::<RapierWorld3d>();
        synced.retain(|(e, _)| rapier.body_handle(*e).is_some());
    }
    if synced.is_empty() {
        return;
    }
    let mut q = world
        .query_filtered::<(&mut Transform, &mut Velocity), (With<RapierBody3d>, Without<Held>)>();
    for (e, snap) in synced {
        if let Ok((mut tf, mut vel)) = q.get_mut(world, e) {
            tf.translation = snap.pos;
            tf.rotation = snap.rot;
            vel.linear = snap.linvel;
            vel.angular = snap.angvel;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use repame_shell::Sim;

    use super::*;
    use crate::maker::player::spawn_player;

    fn sim_with_rapier() -> Sim {
        let mut sim = Sim::with_default_step();
        repame_rapier3d::init_world(&mut sim.world, Vec3::new(0.0, -9.81, 0.0), 1.0);
        repame_rapier3d::register_rapier3d_systems(&mut sim);
        sim.world.insert_resource(MakerMode::Play);
        sim.world.insert_resource(CameraRig::default());
        sim.world.insert_resource(PlayIntent::default());
        sim.world.insert_resource(LevelDocument::default());
        sim.world.insert_resource(RuntimeSolids::default());
        let level = LevelDocument::default();
        spawn_player(&mut sim.world, &level);
        sim
    }

    #[test]
    fn throw_sets_velocity_request_that_reaches_the_body_snapshot() {
        let mut sim = sim_with_rapier();
        let e = sim
            .world
            .spawn((
                Transform::from_translation(Vec3::new(1.0, 2.0, -3.0)),
                Velocity::zero(),
                Throwable,
                Held,
                BodySnapshot3d::default(),
            ))
            .id();
        sim.world.resource_mut::<PlayIntent>().throw_pressed = true;

        pickup_throwables(&mut sim.world);

        let body = sim
            .world
            .get::<RapierBody3d>(e)
            .expect("throw spawns a rigid body");
        assert_eq!(body.kind, BodyKind::Dynamic);
        assert_eq!(body.spawn_pos, Vec3::new(1.0, 2.0, -3.0));
        assert!(
            sim.world.get::<Held>(e).is_none(),
            "throw releases the crate"
        );
        let want = Vec3::new(0.0, 3.5, -14.0);
        let request = *sim
            .world
            .get::<VelocityRequest3d>(e)
            .expect("throw writes a velocity request");
        assert!(
            (request.linear - want).length() < 1e-4,
            "{:?}",
            request.linear
        );
        assert_eq!(request.angular, Vec3::ZERO);
        assert!(
            (sim.world.get::<Velocity>(e).unwrap().linear - want).length() < 1e-4,
            "Velocity mirrors the request for this frame's interaction reads"
        );

        sim.step(Duration::from_millis(17));

        assert!(
            sim.world
                .resource::<RapierWorld3d>()
                .body_handle(e)
                .is_some(),
            "the body was spawned inside the tick"
        );
        let snap = *sim.world.get::<BodySnapshot3d>(e).unwrap();
        assert!(snap.linvel.x.abs() < 1e-3, "{:?}", snap.linvel);
        assert!((snap.linvel.z + 14.0).abs() < 1e-3, "{:?}", snap.linvel);
        assert!(
            (snap.linvel.y - (3.5 - 9.81 / 60.0)).abs() < 1e-2,
            "one gravity step off {:?}",
            snap.linvel
        );
        let consumed = sim.world.get::<VelocityRequest3d>(e).unwrap();
        assert_eq!(
            consumed.linear,
            Vec3::ZERO,
            "request is consumed once applied"
        );
        assert_eq!(consumed.angular, Vec3::ZERO);
        assert!(
            (snap.pos - (Vec3::new(1.0, 2.0, -3.0) + snap.linvel / 60.0)).length() < 1e-2,
            "position advanced by one integration step: {:?}",
            snap.pos
        );
    }
}
