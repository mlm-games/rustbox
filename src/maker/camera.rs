use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;

use super::mode::InputCapture;
use super::player::Player;
use super::MakerCleanup;

use crate::ecosystem::screen_effects::CameraBase3d;

#[derive(Component)]
pub struct WorldCamera;

#[derive(Resource)]
pub struct CameraRig {
    pub focus: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            focus: Vec3::new(0.0, 1.0, 0.0),
            yaw: 0.0,
            pitch: 0.6,
            distance: 18.0,
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

    if !capture.ui_wants_pointer && buttons.pressed(MouseButton::Right) {
        rig.yaw -= delta.x * 0.005;
        rig.pitch = (rig.pitch + delta.y * 0.005).clamp(0.05, 1.5);
    }
    rig.distance = (rig.distance - scroll * 1.5).clamp(4.0, 60.0);

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
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut rig: ResMut<CameraRig>,
    player_q: Query<&Transform, (With<Player>, Without<WorldCamera>)>,
    mut cam: Query<(&mut Transform, &mut CameraBase3d), With<WorldCamera>>,
) {
    let mut delta = Vec2::ZERO;
    for e in motion.read() {
        delta += e.delta;
    }
    let mut scroll = 0.0;
    for e in wheel.read() {
        scroll += e.y;
    }

    if !capture.ui_wants_pointer && buttons.pressed(MouseButton::Right) {
        rig.yaw -= delta.x * 0.005;
        rig.pitch = (rig.pitch + delta.y * 0.005).clamp(0.1, 1.3);
    }
    rig.distance = (rig.distance - scroll).clamp(4.0, 30.0);

    if let Ok(player) = player_q.single() {
        let target = player.translation + Vec3::Y;
        let k = (1.0 - (-12.0 * time.delta_secs()).exp()).clamp(0.0, 1.0);
        rig.focus = rig.focus.lerp(target, k);
    }

    if let Ok((mut t, mut base)) = cam.single_mut() {
        let desired = rig_transform(&rig);
        *t = desired;
        base.translation = desired.translation;
        base.rotation = desired.rotation;
    }
}
