use bevy::prelude::*;

use super::MakerCleanup;
use super::camera::CameraRig;
use super::collision::{move_and_collide, overlaps_kind};
use super::entities_runtime::{DriftPlate, LaunchPad, RuntimeSolids};
use super::level::LevelDocument;
use super::mode::{InputCapture, MakerMode};
use super::rendering::MakerAssets;

use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};

const GRAVITY: f32 = -25.0;
const MOVE_SPEED: f32 = 6.0;
const JUMP_SPEED: f32 = 9.0;
const COYOTE_TIME: f32 = 0.10;
const JUMP_BUFFER: f32 = 0.12;
const MAX_FALL: f32 = -40.0;

#[derive(Component)]
pub struct Player {
    pub velocity: Vec3,
    pub on_ground: bool,
    pub coyote: f32,
    pub jump_buffer: f32,
    pub half_extents: Vec3,
    pub was_on_ground: bool,
    pub fall_speed: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            velocity: Vec3::ZERO,
            on_ground: false,
            coyote: 0.0,
            jump_buffer: 0.0,
            half_extents: Vec3::new(0.3, 0.9, 0.3),
            was_on_ground: true,
            fall_speed: 0.0,
        }
    }
}

pub fn spawn_center(level: &LevelDocument) -> Vec3 {
    let s = level.data.spawn;
    Vec3::new(s[0] as f32 + 0.5, s[1] as f32 + 0.9, s[2] as f32 + 0.5)
}

pub fn spawn_player(commands: &mut Commands, assets: &MakerAssets, level: &LevelDocument) {
    commands.spawn((
        Mesh3d(assets.player_mesh.clone()),
        MeshMaterial3d(assets.player_mat.clone()),
        Transform::from_translation(spawn_center(level)),
        Visibility::Hidden,
        Player::default(),
        MakerCleanup,
    ));
}

pub fn player_controller(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    level: Res<LevelDocument>,
    rig: Res<CameraRig>,
    solids: Res<RuntimeSolids>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    pads: Query<(&Transform, &LaunchPad), Without<Player>>,
    plates: Query<(&Transform, &DriftPlate), Without<Player>>,
    mut q: Query<(Entity, &mut Transform, &mut Player)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let kb = !capture.ui_wants_keyboard;

    for (entity, mut transform, mut player) in &mut q {
        let mut wish = Vec2::ZERO;
        if kb {
            if keys.pressed(KeyCode::KeyW) {
                wish.y += 1.0;
            }
            if keys.pressed(KeyCode::KeyS) {
                wish.y -= 1.0;
            }
            if keys.pressed(KeyCode::KeyA) {
                wish.x -= 1.0;
            }
            if keys.pressed(KeyCode::KeyD) {
                wish.x += 1.0;
            }
        }

        let (sin, cos) = rig.yaw.sin_cos();
        let forward = Vec3::new(-sin, 0.0, -cos);
        let right = Vec3::new(cos, 0.0, -sin);
        let mut horiz = forward * wish.y + right * wish.x;
        if horiz.length_squared() > 1.0 {
            horiz = horiz.normalize();
        }
        player.velocity.x = horiz.x * MOVE_SPEED;
        player.velocity.z = horiz.z * MOVE_SPEED;

        player.coyote = (player.coyote - dt).max(0.0);
        player.jump_buffer = (player.jump_buffer - dt).max(0.0);
        if kb && keys.just_pressed(KeyCode::Space) {
            player.jump_buffer = JUMP_BUFFER;
        }
        if player.jump_buffer > 0.0 && player.coyote > 0.0 {
            player.velocity.y = JUMP_SPEED;
            player.jump_buffer = 0.0;
            player.coyote = 0.0;
            player.on_ground = false;
        }

        // Launch pads (apply velocity but let frames tick cooldown separately)
        for (pad_tf, pad) in &pads {
            let flat = Vec3::new(transform.translation.x, 0.0, transform.translation.z);
            let pad_flat = Vec3::new(pad_tf.translation.x, 0.0, pad_tf.translation.z);
            let on_pad = flat.distance(pad_flat) < 0.7
                && (transform.translation.y - pad_tf.translation.y).abs() < 1.2
                && player.velocity.y <= 0.5;
            if on_pad {
                let dir = Quat::from_rotation_y(pad.yaw_rad) * Vec3::NEG_Z;
                player.velocity = dir * pad.impulse + Vec3::Y * (pad.impulse * 0.35);
                player.on_ground = false;
                player.coyote = 0.0;
                ScreenEffects::add_trauma(&mut trauma, 0.2);
            }
        }

        // Drift plate carry
        let mut carry = Vec3::ZERO;
        for (dtf, drift) in &plates {
            let flat = Vec2::new(
                transform.translation.x - dtf.translation.x,
                transform.translation.z - dtf.translation.z,
            );
            let on = flat.length() < 0.85
                && transform.translation.y >= dtf.translation.y - 0.05
                && transform.translation.y <= dtf.translation.y + 1.4;
            if on {
                carry += drift.carry;
            }
        }
        transform.translation += carry;

        player.velocity.y = (player.velocity.y + GRAVITY * dt).max(MAX_FALL);

        let he = player.half_extents;
        let result = move_and_collide(
            transform.translation,
            he,
            player.velocity * dt + carry,
            &level,
            &solids.boxes,
        );
        if result.hit_x {
            player.velocity.x = 0.0;
        }
        if result.hit_z {
            player.velocity.z = 0.0;
        }
        if result.hit_y {
            player.velocity.y = 0.0;
        }

        if result.on_ground && !player.was_on_ground {
            let impact = (-player.fall_speed).max(0.0);
            if impact > 4.0 {
                Juice::squash_stretch(&mut commands, entity, Vec2::new(1.25, 0.7), 0.12);
                if impact > 10.0 {
                    let amount = ((impact - 10.0) / 40.0).clamp(0.05, 0.35);
                    ScreenEffects::add_trauma(&mut trauma, amount);
                }
            }
            player.fall_speed = 0.0;
        }
        if !result.on_ground {
            player.fall_speed = player.fall_speed.min(player.velocity.y);
        }
        player.was_on_ground = result.on_ground;

        if result.on_ground {
            player.on_ground = true;
            player.coyote = COYOTE_TIME;
        } else {
            player.on_ground = false;
        }
        transform.translation = result.pos;
    }
}

pub fn play_hazard_goal(
    keys: Res<ButtonInput<KeyCode>>,
    level: Res<LevelDocument>,
    mut ui: ResMut<super::ui_bridge::MakerUi>,
    mut q: Query<(&mut Transform, &mut Player)>,
) {
    for (mut transform, mut player) in &mut q {
        let he = player.half_extents;
        let hit_hazard = overlaps_kind(
            transform.translation,
            he,
            &level,
            super::block::BlockKind::Hazard,
        );
        let fell_off = transform.translation.y < -20.0;
        let manual_reset = keys.just_pressed(KeyCode::KeyR);

        if hit_hazard || fell_off || manual_reset {
            if hit_hazard || fell_off {
                ui.deaths += 1;
            }
            transform.translation = spawn_center(&level);
            player.velocity = Vec3::ZERO;
            continue;
        }
    }
}

pub fn sync_mode(
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    mut q: Query<(&mut Transform, &mut Player, &mut Visibility)>,
) {
    if !mode.is_changed() {
        return;
    }
    for (mut transform, mut player, mut vis) in &mut q {
        match *mode {
            MakerMode::Play => {
                *vis = Visibility::Visible;
                transform.translation = spawn_center(&level);
                player.velocity = Vec3::ZERO;
                player.on_ground = false;
                player.coyote = 0.0;
                player.jump_buffer = 0.0;
                player.was_on_ground = true;
                player.fall_speed = 0.0;
            }
            MakerMode::Edit => {
                *vis = Visibility::Hidden;
            }
        }
    }
}
