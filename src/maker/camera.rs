use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;

use super::MakerCleanup;
use super::collision::collide_camera_eye;
use super::entities_runtime::RuntimeSolids;
use super::entity_data::EntityDataExt;
use super::level::LevelDocument;
use super::mode::{InputCapture, SelectionSet};
use super::player::{MoveState, Player};

use game_utils_bevy::screen_effects::CameraBase3d;

#[derive(Component)]
pub struct WorldCamera;

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

fn rig_transform(rig: &CameraRig) -> Transform {
    let rot = Quat::from_euler(EulerRot::YXZ, rig.yaw, -rig.pitch, 0.0);
    let eye = rig.focus + rot * Vec3::new(0.0, 0.0, rig.distance);
    Transform::from_translation(eye).looking_at(rig.focus, Vec3::Y)
}

pub fn spawn_camera(mut commands: Commands, rig: Res<CameraRig>) {
    let tf = rig_transform(&rig);
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 0,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.53, 0.72, 0.92)),
            ..default()
        },
        tf,
        CameraBase3d {
            translation: tf.translation,
            rotation: tf.rotation,
        },
        WorldCamera,
        MakerCleanup,
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::YXZ, -0.6, -0.9, 0.0)),
        MakerCleanup,
    ));
}

pub fn edit_camera_control(
    capture: Res<InputCapture>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut rig: ResMut<CameraRig>,
    level: Res<LevelDocument>,
    mut cam: Query<(&mut Transform, &mut CameraBase3d), With<WorldCamera>>,
) {
    let dt = time.delta_secs();
    let mut delta = Vec2::ZERO;
    for e in motion.read() {
        delta += e.delta;
    }
    let mut scroll = 0.0;
    for e in wheel.read() {
        scroll += e.y;
    }

    if !capture.ui_wants_pointer {
        if buttons.pressed(MouseButton::Right) {
            rig.yaw -= delta.x * 0.005;
            rig.pitch = (rig.pitch + delta.y * 0.005).clamp(0.05, 1.5);
        }
        rig.distance = (rig.distance - scroll * 1.5).clamp(4.0, 60.0);
    }

    if !capture.ui_wants_keyboard {
        let (sin, cos) = rig.yaw.sin_cos();
        let forward = Vec3::new(-sin, 0.0, -cos);
        let right = Vec3::new(cos, 0.0, -sin);
        let mut pan = Vec3::ZERO;
        if keys.pressed(KeyCode::KeyW) {
            pan += forward;
        }
        if keys.pressed(KeyCode::KeyS) {
            pan -= forward;
        }
        if keys.pressed(KeyCode::KeyD) {
            pan += right;
        }
        if keys.pressed(KeyCode::KeyA) {
            pan -= right;
        }
        if keys.pressed(KeyCode::KeyE) {
            pan += Vec3::Y;
        }
        if keys.pressed(KeyCode::KeyQ) {
            pan -= Vec3::Y;
        }
        rig.focus += pan * 12.0 * dt;

        if let Some((min, max)) = level.content_bounds() {
            const PAD: f32 = 4.0;
            let (min, max) = (min.as_vec3(), max.as_vec3() + Vec3::ONE);
            rig.focus.x = rig.focus.x.clamp(min.x - PAD, max.x + PAD);
            rig.focus.z = rig.focus.z.clamp(min.z - PAD, max.z + PAD);
        }
    }

    if let Ok((mut t, mut base)) = cam.single_mut() {
        let desired = rig_transform(&rig);
        *t = desired;
        base.translation = desired.translation;
        base.rotation = desired.rotation;
    }
}

pub fn play_camera_follow(
    capture: Res<InputCapture>,
    time: Res<Time>,
    gamepads: Query<&Gamepad>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut rig: ResMut<CameraRig>,
    level: Res<LevelDocument>,
    solids: Res<RuntimeSolids>,
    player_q: Query<
        (&Transform, &Player, Option<&MoveState>),
        (With<Player>, Without<WorldCamera>),
    >,
    mut cam: Query<(&mut Transform, &mut CameraBase3d), With<WorldCamera>>,
) {
    let dt = time.delta_secs();
    let mut delta = Vec2::ZERO;
    for e in motion.read() {
        delta += e.delta;
    }
    let mut scroll = 0.0;
    for e in wheel.read() {
        scroll += e.y;
    }

    let mut looking = false;
    if !capture.ui_wants_pointer {
        if delta.length_squared() > 0.01 {
            looking = true;
            rig.yaw -= delta.x * rig.look_sensitivity;
            rig.pitch = (rig.pitch + delta.y * rig.look_sensitivity).clamp(0.08, 1.25);
        }
        rig.distance = (rig.distance - scroll * 0.9).clamp(5.0, 22.0);
    }
    for pad in &gamepads {
        let r = pad.right_stick();
        if r.length() > 0.18 {
            looking = true;
            rig.yaw -= r.x * 2.4 * dt;
            rig.pitch = (rig.pitch + r.y * 1.7 * dt).clamp(0.08, 1.25);
        }
    }
    if looking {
        rig.since_manual_look = 0.0;
    } else {
        rig.since_manual_look = (rig.since_manual_look + dt).min(99.0);
    }

    if let Ok((player_tf, player, move_state)) = player_q.single() {
        let vk = (1.0 - (-6.0 * dt).exp()).clamp(0.0, 1.0);
        rig.lead_vel = rig.lead_vel.lerp(player.velocity, vk);
        let flat_lead = Vec3::new(rig.lead_vel.x, 0.0, rig.lead_vel.z).clamp_length_max(6.0);
        let look_ahead = flat_lead * 0.12;
        let vertical_bias = Vec3::Y * (rig.lead_vel.y * 0.02).clamp(-0.25, 0.35);
        let target = player_tf.translation + Vec3::new(0.0, 1.15, 0.0) + look_ahead + vertical_bias;
        let kh = (1.0 - (-9.0 * dt).exp()).clamp(0.0, 1.0);
        let kv = (1.0 - (-4.5 * dt).exp()).clamp(0.0, 1.0);
        rig.focus.x += (target.x - rig.focus.x) * kh;
        rig.focus.z += (target.z - rig.focus.z) * kh;
        rig.focus.y += (target.y - rig.focus.y) * kv;

        if !looking
            && rig.auto_yaw_strength > 0.0
            && rig.since_manual_look > 1.1
            && player.on_ground
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
    }

    if let Ok((mut t, mut base)) = cam.single_mut() {
        let desired = rig_transform(&rig);
        let eye = collide_camera_eye(rig.focus, desired.translation, &level, &solids.solids);
        let collided = Transform::from_translation(eye).looking_at(rig.focus, Vec3::Y);
        let blocked = (collided.translation - rig.focus).length_squared()
            < (t.translation - rig.focus).length_squared() - 1e-6;
        let rate = if blocked { 18.0 } else { 5.0 };
        let ck = (1.0 - (-rate * dt).exp()).clamp(0.0, 1.0);
        t.translation = t.translation.lerp(collided.translation, ck);
        t.rotation = t.rotation.slerp(collided.rotation, ck);
        base.translation = t.translation;
        base.rotation = t.rotation;
    }
}

/// Shift+F frames the current selection. If nothing is selected, it frames
/// the playable level bounds.
pub fn frame_selection_hotkey(
    capture: Res<InputCapture>,
    keys: Res<ButtonInput<KeyCode>>,
    selection: Res<SelectionSet>,
    level: Res<LevelDocument>,
    mut rig: ResMut<CameraRig>,
    mut cam: Query<(&mut Transform, &mut CameraBase3d), With<WorldCamera>>,
) {
    if capture.ui_wants_keyboard {
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if !shift || !keys.just_pressed(KeyCode::KeyF) {
        return;
    }

    let mut points: Vec<Vec3> = Vec::new();

    for &cell in &selection.blocks {
        if level.get_block(cell).is_some() {
            points.push(cell.as_vec3());
            points.push(cell.as_vec3() + Vec3::ONE);
        }
    }

    for &id in &selection.entities {
        if let Some(entity) = level.entity_by_id(id) {
            let c = entity.cell_i().as_vec3();
            points.push(c);
            points.push(c + Vec3::ONE);
        }
    }

    if points.is_empty() {
        let (bounds_min, bounds_max) = level.play_bounds();
        points.push(bounds_min.as_vec3());
        points.push(bounds_max.as_vec3() + Vec3::ONE);
    }

    let mut min = points[0];
    let mut max = points[0];
    for &p in &points {
        min = min.min(p);
        max = max.max(p);
    }

    let center = (min + max) * 0.5;
    let extents = max - min;

    rig.focus = center;
    rig.distance = (extents.length() * 1.35).clamp(8.0, 80.0);

    if let Ok((mut transform, mut base)) = cam.single_mut() {
        let desired = rig_transform(&rig);
        *transform = desired;
        base.translation = desired.translation;
        base.rotation = desired.rotation;
    }
}
