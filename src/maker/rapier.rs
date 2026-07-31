use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

use super::entities_runtime::{DriftPlate, LaunchPad, Seal, TrackFollower};
use super::mode::MakerMode;

pub fn rapier_plugin(app: &mut App) {
    app.add_plugins(RapierPhysicsPlugin::<NoUserData>::default())
        .add_systems(
            Update,
            (sync_drift_plates, apply_launch_impulses, sync_seal_open),
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

/// LaunchPad applies an impulse to the player.
fn apply_launch_impulses(
    mut pads: Query<(&Transform, &mut LaunchPad)>,
    mut player: Query<(&Transform, &mut ExternalImpulse), With<crate::maker::player::Player>>,
) {
    for (pad_tf, mut pad) in &mut pads {
        if pad.cooldown > 0.0 {
            continue;
        }
        for (ptf, mut impulse) in &mut player {
            let flat = Vec3::new(ptf.translation.x, 0.0, ptf.translation.z);
            let pad_flat = Vec3::new(pad_tf.translation.x, 0.0, pad_tf.translation.z);
            if flat.distance(pad_flat) < 0.8
                && (ptf.translation.y - pad_tf.translation.y).abs() < 1.3
            {
                let dir = Quat::from_rotation_y(pad.yaw_rad) * Vec3::NEG_Z;
                impulse.impulse = dir * pad.impulse;
                impulse.torque_impulse = Vec3::ZERO;
                pad.cooldown = 0.45;
            }
        }
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
