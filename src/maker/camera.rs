use bevy_ecs::prelude::{Mut, Resource, World};
use glam::{EulerRot, Quat, Vec2, Vec3};
use repame_view3d::{NEAR, OrbitCamera};

use super::Paused;
use super::collision::collide_camera_eye;
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::{ActionState, MoveState, Player, PlayerTransform};
use super::props::RuntimeSolids;

#[derive(Resource)]
pub struct CameraRig {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// 0 = full manual, 1 = gentle drift behind sustained forward motion.
    /// Never snaps while steering/strafing; that fight is what made controls
    /// feel stiff.
    pub auto_yaw_strength: f32,
    pub look_sensitivity: f32,
    /// Seconds since the last manual look (mouse / right stick). Auto-drift
    /// stays parked while this is small so it can't fight the player's input.
    pub since_manual_look: f32,
    /// Low-passed player velocity driving the soft position lead (prevents
    /// raw-velocity jitter from feeding back into the frame).
    pub lead_vel: Vec3,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.0,
            pitch: 0.42,
            distance: 12.0,
            auto_yaw_strength: 0.22,
            look_sensitivity: 0.0045,
            since_manual_look: 99.0,
            lead_vel: Vec3::ZERO,
        }
    }
}

fn rig_eye(rig: &CameraRig) -> Vec3 {
    let rot = Quat::from_euler(EulerRot::YXZ, rig.yaw, -rig.pitch, 0.0);
    rig.focus + rot * Vec3::new(0.0, 0.0, rig.distance)
}

pub fn play_camera_follow(world: &mut World, dt: f32, looking: bool, cam: &mut OrbitCamera) {
    if *world.resource::<MakerMode>() != MakerMode::Play || world.resource::<Paused>().0 {
        return;
    }
    world.resource_scope(|world, mut rig: Mut<CameraRig>| {
        if looking {
            rig.since_manual_look = 0.0;
        } else {
            rig.since_manual_look = (rig.since_manual_look + dt).min(99.0);
        }

        let mut q = world.query::<(&PlayerTransform, &Player, Option<&MoveState>)>();
        let Ok((player_tf, player, move_state)) = q.single(world) else {
            return;
        };
        // Course-style: scripted motion (launch pads, cannons, slams) must not
        // yank the frame. Freeze the lead while scripted, decay it instead.
        let scripted = player.launch > 0.0 || player.slamming;
        if scripted {
            rig.lead_vel *= 1.0 - (1.0 - (-3.0 * dt).exp()).clamp(0.0, 1.0);
        } else {
            let vk = (1.0 - (-6.0 * dt).exp()).clamp(0.0, 1.0);
            rig.lead_vel = rig.lead_vel.lerp(player.velocity, vk);
        }
        let flat_lead = Vec3::new(rig.lead_vel.x, 0.0, rig.lead_vel.z).clamp_length_max(6.0);
        let look_ahead = flat_lead * 0.12;
        let vertical_bias = Vec3::Y * (rig.lead_vel.y * 0.02).clamp(-0.25, 0.35);
        let target = player_tf.translation + Vec3::new(0.0, 1.15, 0.0) + look_ahead + vertical_bias;
        let kh = (1.0 - (-9.0 * dt).exp()).clamp(0.0, 1.0);
        let kv = (1.0 - (-4.5 * dt).exp()).clamp(0.0, 1.0);
        rig.focus.x += (target.x - rig.focus.x) * kh;
        rig.focus.z += (target.z - rig.focus.z) * kh;
        rig.focus.y += (target.y - rig.focus.y) * kv;

        let running = move_state
            .map(|m| m.action == ActionState::Run)
            .unwrap_or(player.on_ground);
        if !looking
            && rig.auto_yaw_strength > 0.0
            && rig.since_manual_look > 1.6
            && player.on_ground
            && running
            && !scripted
        {
            let wish = move_state.map(|m| m.wish_dir).unwrap_or(Vec3::ZERO);
            let forward_push = {
                let (s, c) = rig.yaw.sin_cos();
                let fwd = Vec3::new(-s, 0.0, -c);
                wish.dot(fwd)
            };
            let speed = Vec2::new(player.velocity.x, player.velocity.z).length();
            if forward_push > 0.65 && wish.length() > 0.5 && speed > 2.5 {
                let dir = Vec3::new(player.velocity.x, 0.0, player.velocity.z);
                if dir.length_squared() > 6.25 {
                    let target_yaw = (-dir.x).atan2(-dir.z);
                    let mut dy = target_yaw - rig.yaw;
                    while dy > std::f32::consts::PI {
                        dy -= std::f32::consts::TAU;
                    }
                    while dy < -std::f32::consts::PI {
                        dy += std::f32::consts::TAU;
                    }
                    if dy.abs() > 0.06 {
                        let ak = (rig.auto_yaw_strength * 1.4 * dt).clamp(0.0, 1.0);
                        rig.yaw += dy * ak;
                    }
                }
            }
        }
    });

    let rig = world.resource::<CameraRig>();
    let level = world.resource::<LevelDocument>();
    let solids = world.resource::<RuntimeSolids>();
    let desired = rig_eye(rig);
    let focus = rig.focus;
    let current = cam.eye();
    let collided = collide_camera_eye(focus, desired, level, &solids.solids);
    let blocked = (collided - focus).length_squared() < (current - focus).length_squared() - 1e-6;
    let collided_dist = (collided - focus).length();
    cam.target = focus;
    cam.yaw = std::f32::consts::FRAC_PI_2 - rig.yaw;
    cam.pitch = rig.pitch;
    if blocked {
        // Snap: lerping old→safe sweeps through corner geometry.
        cam.dist = collided_dist;
    } else {
        let ck = (1.0 - (-5.0 * dt).exp()).clamp(0.0, 1.0);
        cam.dist = cam.dist + (collided_dist - cam.dist) * ck;
    }
    cam.dist = cam.dist.clamp(NEAR, 22.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_orbit_matches_main_rig_under_same_delta() {
        let mut rig = CameraRig::default();
        rig.focus = Vec3::new(3.0, 1.0, -2.0);
        rig.yaw = 0.8;
        rig.pitch = 0.7;
        let mut cam = OrbitCamera {
            target: rig.focus,
            yaw: std::f32::consts::FRAC_PI_2 - rig.yaw,
            pitch: rig.pitch,
            dist: rig.distance,
            fov_y_deg: 45.0,
        };
        let (dx, dy) = (37.0, -11.0);

        rig.yaw -= dx * 0.005;
        rig.pitch = (rig.pitch + dy * 0.005).clamp(0.05, 1.5);
        cam.yaw += dx * 0.005;
        cam.pitch = (cam.pitch + dy * 0.005).clamp(0.05, 1.5);

        assert!((cam.yaw - (std::f32::consts::FRAC_PI_2 - rig.yaw)).abs() < 1e-6);
        assert!((cam.pitch - rig.pitch).abs() < 1e-6);
        assert!((cam.eye() - rig_eye(&rig)).length() < 1e-5);
    }
}
