use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use super::entities_runtime::Seal;
use super::mode::MakerMode;
use super::player::Player;

pub fn rapier_plugin(app: &mut App) {
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_systems(
            Update,
            (
                sync_seal_open,
                move_held_objects,
                pickup_throwables.after(move_held_objects),
            ),
        );
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
/// player, tracking the camera heading. Its collider is stripped while held so
/// Rapier can't shove the player with the kinematic body.
fn move_held_objects(
    rig: Res<super::camera::CameraRig>,
    player: Query<&Transform, With<Player>>,
    mut commands: Commands,
    mut held: Query<
        (Entity, &mut Transform, &mut Velocity, Option<&Collider>),
        (With<Held>, Without<Player>),
    >,
) {
    let Ok(ptf) = player.single() else {
        return;
    };
    let (sin, cos) = rig.yaw.sin_cos();
    let forward = Vec3::new(-sin, 0.0, -cos);
    for (e, mut tf, mut vel, col) in &mut held {
        tf.translation = ptf.translation + forward * 1.2 - Vec3::Y * 0.15;
        vel.linear = Vec3::ZERO;
        vel.angular = Vec3::ZERO;
        if col.is_some() {
            commands.entity(e).remove::<Collider>();
        }
    }
}

/// F (or Right Trigger) near a Throwable picks it up (turns it kinematic);
/// F while holding throws it (switches back to a dynamic body with impulse and
/// restores its collider).
fn pickup_throwables(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mode: Res<MakerMode>,
    rig: Res<super::camera::CameraRig>,
    mut commands: Commands,
    player: Query<&Transform, With<Player>>,
    crates: Query<(Entity, &Transform), (With<Throwable>, Without<Held>)>,
    held_q: Query<Entity, (With<Throwable>, With<Held>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let pad_throw = gamepads.iter().any(|g| g.just_pressed(GamepadButton::RightTrigger));
    if !keys.just_pressed(KeyCode::KeyF) && !pad_throw {
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
            .remove::<Held>()
            .insert(Collider::cuboid(0.4, 0.4, 0.4))
            .insert(Velocity {
                linear: forward * 14.0 + Vec3::Y * 3.5,
                angular: Vec3::ZERO,
            });
        return;
    }

    // Otherwise pick up the nearest crate in front of the player within reach.
    let mut best: Option<(Entity, f32)> = None;
    for (e, tf) in &crates {
        let to = (tf.translation - ptf.translation).normalize_or_zero();
        let facing = forward.dot(to);
        let d = ptf.translation.distance(tf.translation);
        if d < 1.6 && facing > 0.15 && best.map_or(true, |(_, bd)| d < bd) {
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
