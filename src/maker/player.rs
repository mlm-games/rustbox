use std::collections::HashMap;

use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use super::MakerCleanup;
use super::camera::CameraRig;
use super::collision::{ledge_grip, move_and_collide, overlaps_kind, slope_slide};
use super::entities_runtime::{DriftPlate, LaunchPad, ModelAnim, ModelMaterial, RuntimeSolids};
use super::level::LevelDocument;
use super::mode::{InputCapture, MakerMode};
use super::rendering::MakerAssets;

use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};

const GRAVITY: f32 = -25.0;
const WATER_GRAVITY: f32 = -4.5;
const MOVE_SPEED: f32 = 6.0;
const SLIDE_SPEED: f32 = 4.0;
pub const JUMP_SPEED: f32 = 9.0;
const SLIDE_JUMP_SPEED: f32 = 11.5;
const COYOTE_TIME: f32 = 0.10;
const JUMP_BUFFER: f32 = 0.12;
const MAX_FALL: f32 = -40.0;
const MAX_FALL_WATER: f32 = -8.0;
const SWIM_SPEED: f32 = 4.5;
const SLAM_SPEED: f32 = -34.0;

#[derive(Component)]
pub struct Player {
    pub velocity: Vec3,
    pub on_ground: bool,
    pub coyote: f32,
    pub jump_buffer: f32,
    pub half_extents: Vec3,
    pub was_on_ground: bool,
    pub fall_speed: f32,
    pub launch: f32,
    /// Currently slamming (air-drop strike). Sets a fast downward velocity and
    /// makes the landing a heavy impact.
    pub slamming: bool,
    /// Currently gripped onto a ledge lip (climbing/cling state).
    pub gripping: bool,
    /// The world height the player can pull up to while gripping.
    pub grip_top: f32,
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
            launch: 0.0,
            slamming: false,
            gripping: false,
            grip_top: 0.0,
        }
    }
}

pub fn spawn_center(level: &LevelDocument) -> Vec3 {
    let s = level.data.spawn;
    Vec3::new(s[0] as f32 + 0.5, s[1] as f32 + 0.9, s[2] as f32 + 0.5)
}

pub fn reset_player(
    transform: &mut Transform,
    player: &mut Player,
    vis: &mut Visibility,
    level: &LevelDocument,
) {
    *vis = Visibility::Visible;
    transform.translation = spawn_center(level);
    player.velocity = Vec3::ZERO;
    player.on_ground = false;
    player.coyote = 0.0;
    player.jump_buffer = 0.0;
    player.was_on_ground = true;
    player.fall_speed = 0.0;
    player.launch = 0.0;
    player.slamming = false;
    player.gripping = false;
    player.grip_top = 0.0;
}

pub fn spawn_player(commands: &mut Commands, assets: &MakerAssets, level: &LevelDocument) {
    let root = commands
        .spawn((
            Transform::from_translation(spawn_center(level)),
            Visibility::Hidden,
            Player::default(),
            MakerCleanup,
        ))
        .id();
    commands.entity(root).with_children(|p| {
        p.spawn((
            WorldAssetRoot(assets.player_scene.clone()),
            MakerCleanup,
            ModelMaterial(assets.player_material.clone()),
            ModelAnim {
                source: "player",
                idle: "Idle",
                run: Some("Run"),
                air: Some("Jump"),
                player: None,
                started: false,
                nodes: HashMap::new(),
                state: None,
            },
            Transform::from_translation(Vec3::Y * -0.9).with_scale(Vec3::splat(0.6)),
        ));
    });
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
        let crouching = kb && keys.pressed(KeyCode::ShiftLeft);
        let crouch_pressed = kb && keys.just_pressed(KeyCode::ShiftLeft);
        let he = player.half_extents;
        let slope = if player.on_ground {
            slope_slide(&level, transform.translation, he)
        } else {
            None
        };
        let mut sliding = slope.is_some();
        if player.launch <= 0.0 && !sliding {
            player.velocity.x = horiz.x * MOVE_SPEED;
            player.velocity.z = horiz.z * MOVE_SPEED;
        }
        if let Some(dir) = slope {
            if player.launch <= 0.0 {
                player.velocity.x = dir.x * SLIDE_SPEED;
                player.velocity.z = dir.y * SLIDE_SPEED;
            }
        } else {
            sliding = false;
        }

        player.coyote = (player.coyote - dt).max(0.0);
        player.jump_buffer = (player.jump_buffer - dt).max(0.0);
        player.launch = (player.launch - dt).max(0.0);
        let underwater = level.is_underwater_point(transform.translation);
        if kb && keys.just_pressed(KeyCode::Space) {
            player.jump_buffer = JUMP_BUFFER;
        }
        if underwater {
            // Swim: hold jump to rise, light gravity, soft terminal fall.
            if kb && keys.pressed(KeyCode::Space) {
                player.velocity.y = SWIM_SPEED;
            }
        } else if player.jump_buffer > 0.0 && player.coyote > 0.0 {
            // A slide-jump launches off the slope with extra height while
            // keeping the slide's horizontal momentum.
            player.velocity.y = if sliding {
                SLIDE_JUMP_SPEED
            } else {
                JUMP_SPEED
            };
            player.jump_buffer = 0.0;
            player.coyote = 0.0;
            player.on_ground = false;
        }

        if !underwater && crouch_pressed && !player.on_ground && player.velocity.y <= 2.0 {
            player.velocity.y = SLAM_SPEED;
            player.velocity.x = 0.0;
            player.velocity.z = 0.0;
            player.slamming = true;
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
                player.launch = 0.9;
                player.on_ground = false;
                player.coyote = 0.0;
                ScreenEffects::add_trauma(&mut trauma, 0.2);
            }
        }

        let he = player.half_extents;
        let underwater = level.is_underwater_point(transform.translation);
        let gravity = if underwater { WATER_GRAVITY } else { GRAVITY };
        let max_fall = if underwater { MAX_FALL_WATER } else { MAX_FALL };
        player.velocity.y = (player.velocity.y + gravity * dt).max(max_fall);

        let move_he = if crouching {
            Vec3::new(he.x, he.y * 0.55, he.z)
        } else {
            he
        };
        let result = move_and_collide(
            transform.translation,
            move_he,
            player.velocity * dt,
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

        // One-way plate riding: probe the plate top under our feet after the
        // move and carry the player with the plate's motion.
        let mut pos = result.pos;
        let feet_y = pos.y - he.y;
        let prev_feet = feet_y - player.velocity.y * dt;
        let mut on_plate = false;
        for (dtf, drift) in &plates {
            let top = dtf.translation.y + 0.125;
            let over =
                (pos.x - dtf.translation.x).abs() < 1.0 && (pos.z - dtf.translation.z).abs() < 1.0;
            if over
                && player.velocity.y <= 0.0
                && prev_feet >= top - 0.05
                && feet_y <= top + 0.05
                && feet_y >= top - 1.5
            {
                pos.x += drift.carry.x;
                pos.z += drift.carry.z;
                pos.y = top + he.y;
                player.velocity.y = 0.0;
                on_plate = true;
                break;
            }
        }
        transform.translation = pos;

        if player.gripping {
            if kb && (keys.just_pressed(KeyCode::Space) || keys.pressed(KeyCode::KeyW)) {
                // Climb up onto the ledge.
                transform.translation.y = player.grip_top + he.y;
                player.gripping = false;
                player.grip_top = 0.0;
                player.coyote = COYOTE_TIME;
                player.velocity = Vec3::ZERO;
            } else if kb && keys.pressed(KeyCode::KeyS) {
                player.gripping = false;
                player.grip_top = 0.0;
            } else if let Some(g) = ledge_grip(&level, transform.translation, he, player.velocity) {
                // Keep holding the (still-present) lip.
                transform.translation.y = g.wall_top - he.y + 0.05;
                player.velocity = Vec3::ZERO;
            } else {
                player.gripping = false;
                player.grip_top = 0.0;
            }
        } else if false && !result.on_ground && player.velocity.y <= 0.0 && !underwater && !player.slamming {
            // Ledge grab is disabled for now: leads to buggy interactions with most blocks.
            if let Some(g) = ledge_grip(&level, transform.translation, he, player.velocity) {
                transform.translation.y = g.wall_top - he.y + 0.05;
                player.gripping = true;
                player.grip_top = g.wall_top;
                player.velocity = Vec3::ZERO;
            }
        }

        let was_grounded = result.on_ground || on_plate || player.gripping;
        if was_grounded && !player.was_on_ground {
            let impact = (-player.fall_speed).max(0.0);
            if player.slamming {
                // A slam landing is always a heavy impact.
                let amount = (impact / 40.0).clamp(0.15, 0.4);
                Juice::squash_stretch(&mut commands, entity, Vec2::new(1.4, 0.6), 0.15);
                ScreenEffects::add_trauma(&mut trauma, amount);
            } else if impact > 4.0 {
                Juice::squash_stretch(&mut commands, entity, Vec2::new(1.25, 0.7), 0.12);
                if impact > 10.0 {
                    let amount = ((impact - 10.0) / 40.0).clamp(0.05, 0.35);
                    ScreenEffects::add_trauma(&mut trauma, amount);
                }
            }
            player.slamming = false;
            player.fall_speed = 0.0;
        }
        if !was_grounded {
            player.fall_speed = player.fall_speed.min(player.velocity.y);
        }
        player.was_on_ground = was_grounded;

        if was_grounded {
            player.on_ground = true;
            player.coyote = COYOTE_TIME;
            player.launch = 0.0;
        } else {
            player.on_ground = false;
        }
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
            MakerMode::Play => reset_player(&mut transform, &mut player, &mut vis, &level),
            MakerMode::Edit => {
                *vis = Visibility::Hidden;
            }
        }
    }
}
