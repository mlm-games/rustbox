use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use super::entities_runtime::{DriftPlate, Seal, TrackFollower};
use super::mode::MakerMode;
use super::player::Player;

pub fn rapier_plugin(app: &mut App) {
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_systems(
            Update,
            (
                sync_drift_plates,
                sync_seal_open,
                move_held_objects,
                pickup_throwables.after(move_held_objects),
            ),
        );
}

/// Make DriftPlate a kinematic body that follows the lerp path.
fn sync_drift_plates(
    mode: Res<MakerMode>,
    mut q: Query<(&mut Transform, &mut Velocity, &DriftPlate), Without<TrackFollower>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    for (mut tf, mut vel, drift) in &mut q {
        let phase = (drift.t % (drift.period * 2.0)) / drift.period;
        let s = if phase <= 1.0 { phase } else { 2.0 - phase };
        let target = drift.a.lerp(drift.b, s * s * (3.0 - 2.0 * s));
        let delta = target - tf.translation;
        vel.linear = delta / 0.016;
        tf.translation = target;
    }
}

/// Remove Seal collider when opened.
fn sync_seal_open(mut commands: Commands, q: Query<(Entity, &Seal), Changed<Seal>>) {
    for (e, seal) in &q {
        if seal.open {
            commands.entity(e).remove::<Collider>();
        }
    }
}

/// A physics crate (TossCrate) that can be picked up and thrown (F).
#[derive(Component)]
pub struct Throwable;

/// Marks a crate currently held in front of the player (kinematic body).
#[derive(Component)]
pub struct Held;

/// While a crate is held it becomes a kinematic body parked in front of the
/// player, tracking the camera heading.
fn move_held_objects(
    rig: Res<super::camera::CameraRig>,
    player: Query<&Transform, With<Player>>,
    mut held: Query<(Entity, &mut Transform, &mut Velocity), With<Held>>,
) {
    let Ok(ptf) = player.single() else {
        return;
    };
    let (sin, cos) = rig.yaw.sin_cos();
    let forward = Vec3::new(-sin, 0.0, -cos);
    for (_, mut tf, mut vel) in &mut held {
        tf.translation = ptf.translation + forward * 1.2 - Vec3::Y * 0.15;
        vel.linear = Vec3::ZERO;
        vel.angular = Vec3::ZERO;
    }
}

/// F near a Throwable picks it up (turns it kinematic); F while holding throws
/// it (switches back to a dynamic body with an impulse).
fn pickup_throwables(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<MakerMode>,
    rig: Res<super::camera::CameraRig>,
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    crates: Query<(Entity, &Transform), With<Throwable>>,
    held_q: Query<Entity, (With<Throwable>, With<Held>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Ok(ptf) = player.single() else {
        return;
    };
    let (sin, cos) = rig.yaw.sin_cos();
    let forward = Vec3::new(-sin, 0.0, -cos);

    // Held crate first: throw it.
    for e in held_q.iter() {
        commands
            .entity(e)
            .insert(RigidBody::Dynamic)
            .remove::<Held>();
        commands.entity(e).insert(Velocity {
            linear: forward * 14.0 + Vec3::Y * 3.5,
            angular: Vec3::ZERO,
        });
        return;
    }

    // Otherwise pick up the nearest crate within reach.
    let mut best: Option<(Entity, f32)> = None;
    for (e, tf) in &crates {
        let d = ptf.translation.distance(tf.translation);
        if d < 1.6 && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((e, d));
        }
    }
    if let Some((e, _)) = best {
        commands
            .entity(e)
            .insert((RigidBody::KinematicPositionBased, Held))
            .insert(Velocity {
                linear: Vec3::ZERO,
                angular: Vec3::ZERO,
            });
    }
}
