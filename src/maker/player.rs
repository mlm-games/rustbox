use std::collections::HashMap;

use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use super::MakerCleanup;
use super::block::BlockKind;
use super::camera::CameraRig;
use super::collision::{
    floor_normal_at, ground_height, ledge_grip, move_and_collide, overlaps_kind, slope_slide,
    stand_headroom,
};
use super::entities_runtime::{DriftPlate, LaunchPad, ModelAnim, ModelMaterial, RuntimeSolids};
use super::entity_data::LevelEntityId;
use super::interactive_blocks::OnOffState;
use super::level::LevelDocument;
use super::mode::{InputCapture, MakerMode};
use super::rendering::MakerAssets;

use game_utils_bevy::juice::Juice;
use game_utils_bevy::screen_effects::{ScreenEffects, Trauma};

/// Default jump impulse (shared with stomp bounce etc.).
pub const JUMP_SPEED: f32 = 9.0;

const GROUND_STEP_MAX: f32 = 0.55;

/// Central movement tunables (Bevy resource). Tune here instead of scattering
/// magic numbers.
#[derive(Resource, Clone, Debug)]
pub struct MoveTuning {
    pub gravity: f32,
    pub water_gravity: f32,
    pub move_speed: f32,
    pub crouch_speed: f32,
    pub slide_speed: f32,
    pub jump_speed: f32,
    pub slide_jump_speed: f32,
    pub coyote_time: f32,
    pub jump_buffer: f32,
    pub max_fall: f32,
    pub max_fall_water: f32,
    pub swim_speed: f32,
    pub slam_speed: f32,
    /// Quake-style accel coefficients (higher = snappier).
    pub ground_accel: f32,
    pub ground_friction: f32,
    pub air_accel: f32,
    pub air_friction: f32,
    pub swim_accel: f32,
    pub swim_friction: f32,
    /// Friction never treats speed below this as "already stopped".
    pub stop_speed: f32,
    /// Min floor-normal.y to count as walkable (else slide).
    pub walkable_normal_y: f32,
    pub half_extents: Vec3,
    pub launch_lock: f32,
}

impl Default for MoveTuning {
    fn default() -> Self {
        Self {
            gravity: -25.0,
            water_gravity: -4.5,
            move_speed: 6.0,
            crouch_speed: 3.25,
            slide_speed: 4.0,
            jump_speed: JUMP_SPEED,
            slide_jump_speed: 11.5,
            coyote_time: 0.10,
            jump_buffer: 0.12,
            max_fall: -40.0,
            max_fall_water: -8.0,
            swim_speed: 4.5,
            slam_speed: -34.0,
            // ~full speed in a few frames on ground; softer in air.
            ground_accel: 32.0,
            ground_friction: 14.0,
            air_accel: 6.0,
            air_friction: 0.2,
            swim_accel: 8.0,
            swim_friction: 4.0,
            stop_speed: 1.5,
            walkable_normal_y: 0.7,
            half_extents: Vec3::new(0.3, 0.9, 0.3),
            launch_lock: 0.9,
        }
    }
}

/// Coarse locomotion / ability state (not a full action graph).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ActionState {
    #[default]
    Run,
    Air,
    Swim,
    Slam,
    Launch,
}

/// Whether the player is hanging from a hangable underside (thin conveyor /
/// hang rail), holding E.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlayerMoveMode {
    #[default]
    Normal,
    Hanging,
}

#[derive(Component, Clone, Debug)]
pub struct MoveState {
    pub action: ActionState,
    pub floor_normal: Vec3,
    pub wall_normal: Vec3,
    pub grounded: bool,
    pub sliding: bool,
    pub wish_dir: Vec3,
}

impl Default for MoveState {
    fn default() -> Self {
        Self {
            action: ActionState::Run,
            floor_normal: Vec3::Y,
            wall_normal: Vec3::ZERO,
            grounded: false,
            sliding: false,
            wish_dir: Vec3::ZERO,
        }
    }
}

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
    /// Currently gripped onto a ledge lip (climb/cling state).
    pub gripping: bool,
    /// The world height of the lip the player hangs on.
    pub grip_top: f32,
    /// Validated pull-up center for the ledge climb.
    pub grip_mantle: Vec3,
    /// World location of the lip, re-checked each frame while hanging.
    pub grip_anchor: Vec3,
    /// Seconds until a new grab is allowed (drop/re-climb jank lock).
    pub grip_cooldown: f32,
    /// How long the player has been hanging.
    pub grip_time: f32,
    /// True while the collider is at crouch height (key or forced by ceiling).
    pub crouched: bool,
    /// Current respawn position for this run. Starts at spawn, then moves to the
    /// last touched checkpoint.
    pub respawn_point: Vec3,
    /// Active checkpoint id for this run, if any.
    pub checkpoint_id: Option<LevelEntityId>,
    /// Keys held this run, by link channel (1-9).
    pub keys: [u8; 10],
    /// Extra hits before death; HealOrb adds 1, max 3.
    pub armor: u8,
    /// Brief post-hit invulnerability (seconds remaining).
    pub invuln: f32,
    /// Speed boost timer (seconds remaining).
    pub speed_boost: f32,
    /// Hanging from a hangable underside (thin conveyor / hang rail).
    pub move_mode: PlayerMoveMode,
    /// Seconds until a new hang grab is allowed after releasing.
    pub hang_cooldown: f32,
}

impl Default for Player {
    fn default() -> Self {
        let t = MoveTuning::default();
        Self {
            velocity: Vec3::ZERO,
            on_ground: false,
            coyote: 0.0,
            jump_buffer: 0.0,
            half_extents: t.half_extents,
            was_on_ground: true,
            fall_speed: 0.0,
            launch: 0.0,
            slamming: false,
            gripping: false,
            grip_top: 0.0,
            grip_mantle: Vec3::ZERO,
            grip_anchor: Vec3::ZERO,
            grip_cooldown: 0.0,
            grip_time: 0.0,
            crouched: false,
            respawn_point: Vec3::ZERO,
            checkpoint_id: None,
            keys: [0; 10],
            armor: 0,
            invuln: 0.0,
            speed_boost: 0.0,
            move_mode: PlayerMoveMode::Normal,
            hang_cooldown: 0.0,
        }
    }
}

pub fn spawn_center(level: &LevelDocument) -> Vec3 {
    let s = level.data.spawn;
    Vec3::new(s[0] as f32 + 0.5, s[1] as f32 + 0.9, s[2] as f32 + 0.5)
}

fn ground_block(level: &LevelDocument, wx: f32, wz: f32) -> Option<BlockKind> {
    let cx = wx.floor() as i32;
    let cz = wz.floor() as i32;
    let from = wx.max(wz).max(0.0) as i32 + 8;
    for y in ((-512 + 8)..=from).rev() {
        let cell = IVec3::new(cx, y, cz);
        if let Some(b) = level.get_block(cell) {
            if level.kind_is_solid(b.kind) {
                return Some(b.kind);
            }
        } else if level.boundary_solid(cell) {
            return None;
        }
    }
    None
}

fn ground_block_rot(level: &LevelDocument, wx: f32, wz: f32) -> Option<u8> {
    let cx = wx.floor() as i32;
    let cz = wz.floor() as i32;
    let from = wx.max(wz).max(0.0) as i32 + 8;
    for y in ((-512 + 8)..=from).rev() {
        let cell = IVec3::new(cx, y, cz);
        if let Some(b) = level.get_block(cell) {
            if level.kind_is_solid(b.kind) {
                return Some(b.rot);
            }
        } else if level.boundary_solid(cell) {
            return None;
        }
    }
    None
}

/// A hangable underside directly above the player's head, if any. Returns the
/// cell and the world height of the slab's bottom surface.
fn find_hang_surface(level: &LevelDocument, pos: Vec3, he: Vec3) -> Option<(IVec3, f32)> {
    let head = pos.y + he.y;
    let cx = pos.x.floor() as i32;
    let cz = pos.z.floor() as i32;
    // Check the column the head is in and one above (in case the head just
    // crossed a cell boundary).
    let top = head.floor() as i32;
    for y in (top - 1..=top + 1).rev() {
        let cell = IVec3::new(cx, y, cz);
        if let Some(b) = level.get_block(cell)
            && b.kind.is_solid()
            && b.kind.has_hangable_underside()
            && let Some(bottom) = super::collision::surface_bottom_height(b, pos.x, pos.z)
        {
            // Only grab while the head is roughly against the underside.
            if (bottom - head).abs() <= 0.35 {
                return Some((cell, bottom));
            }
        }
    }
    None
}

pub fn respawn_player(
    transform: &mut Transform,
    player: &mut Player,
    move_state: &mut MoveState,
    vis: &mut Visibility,
    level: &LevelDocument,
) {
    *vis = Visibility::Visible;
    let spawn = if player.checkpoint_id.is_some() {
        player.respawn_point
    } else {
        spawn_center(level)
    };
    transform.translation = spawn;
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
    player.grip_mantle = Vec3::ZERO;
    player.grip_anchor = Vec3::ZERO;
    player.grip_cooldown = 0.0;
    player.grip_time = 0.0;
    player.crouched = false;
    *move_state = MoveState::default();
}

pub fn reset_player_run(
    transform: &mut Transform,
    player: &mut Player,
    move_state: &mut MoveState,
    vis: &mut Visibility,
    level: &LevelDocument,
) {
    player.keys = [0; 10];
    player.armor = 0;
    player.invuln = 0.0;
    player.speed_boost = 0.0;
    player.respawn_point = spawn_center(level);
    player.checkpoint_id = None;
    respawn_player(transform, player, move_state, vis, level);
}

pub fn spawn_player(commands: &mut Commands, assets: &MakerAssets, level: &LevelDocument) {
    let root = commands
        .spawn((
            Transform::from_translation(spawn_center(level)),
            Visibility::Hidden,
            Player::default(),
            MoveState::default(),
            MakerCleanup,
        ))
        .id();
    commands.entity(root).with_children(|p| {
        p.spawn((
            WorldAssetRoot(assets.player_scene.clone()),
            MakerCleanup,
            Visibility::default(),
            ModelMaterial::fallback(assets.player_material.clone()),
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

/// Platformer-style move on XZ: approach wish velocity while steering; friction
/// only when no wish. Reverse cancels opposing momentum first (no moonwalk lag).
fn apply_accel_friction(
    vel: &mut Vec3,
    wish_dir: Vec3,
    max_speed: f32,
    accel: f32,
    friction: f32,
    stop_speed: f32,
    dt: f32,
) {
    let mut h = Vec3::new(vel.x, 0.0, vel.z);
    let wish_len = wish_dir.length();

    if wish_len > 1e-5 && max_speed > 0.0 {
        let wdir = wish_dir / wish_len;
        let target = wdir * max_speed;
        let along = h.dot(wdir);
        // Opposing momentum: drop it immediately (classic platformer
        // turnaround), then accelerate into the new direction. Stops the
        // "backward slow" feel.
        if along < 0.0 {
            h -= wdir * along; // along -> 0; keeps only the perpendicular part
        }

        let rate = accel * max_speed;
        let max_step = rate * dt;
        let delta = target - h;
        let dlen = delta.length();
        if dlen <= max_step || dlen < 1e-8 {
            h = target;
        } else {
            h += delta * (max_step / dlen);
        }
        let spd = h.length();
        if spd > max_speed * 1.05 {
            h *= (max_speed * 1.05) / spd;
        }
    } else if h.length_squared() > 1e-10 {
        let speed = h.length();
        let control = speed.max(stop_speed);
        let drop = control * friction * dt;
        let new_speed = (speed - drop).max(0.0);
        h *= new_speed / speed;
    } else {
        h = Vec3::ZERO;
    }

    vel.x = h.x;
    vel.z = h.z;
}

pub fn player_controller(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    level: Res<LevelDocument>,
    rig: Res<CameraRig>,
    solids: Res<RuntimeSolids>,
    tuning: Res<MoveTuning>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
    pads: Query<(&Transform, &LaunchPad), Without<Player>>,
    plates: Query<(&Transform, &DriftPlate), Without<Player>>,
    onoff: Res<OnOffState>,
    mut q: Query<(Entity, &mut Transform, &mut Player, &mut MoveState)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let kb = !capture.ui_wants_keyboard;
    let tuning = &*tuning;

    for (entity, mut transform, mut player, mut move_state) in &mut q {
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
        move_state.wish_dir = horiz;

        let want_crouch = kb && keys.pressed(KeyCode::ShiftLeft);
        let crouch_pressed = keys.just_pressed(KeyCode::ShiftLeft);
        let he = player.half_extents;
        let underwater = level.is_underwater_point(transform.translation);

        const CROUCH_FACTOR: f32 = 0.55;
        let crouch_he_y = he.y * CROUCH_FACTOR;

        // Feet from last frame's collider height (keeps plant when height
        // changes across crouch/stand transitions).
        let feet_y = transform.translation.y - if player.crouched { crouch_he_y } else { he.y };

        let can_stand = stand_headroom(
            &level,
            feet_y,
            transform.translation.x,
            transform.translation.z,
            he,
            &solids.solids,
        );
        let effectively_crouching = want_crouch || !can_stand;
        player.crouched = effectively_crouching;

        let move_he = if effectively_crouching {
            Vec3::new(he.x, crouch_he_y, he.z)
        } else {
            he
        };
        transform.translation.y = feet_y + move_he.y;

        // Hanging: hold E under a hangable underside (thin conveyor / hang
        // rail). Overrides gravity while active.
        let hang_key = kb && keys.pressed(KeyCode::KeyE);
        player.hang_cooldown = (player.hang_cooldown - dt).max(0.0);
        if player.move_mode == PlayerMoveMode::Hanging {
            match find_hang_surface(&level, transform.translation, he) {
                Some((_, bottom)) => {
                    if !hang_key || underwater {
                        player.move_mode = PlayerMoveMode::Normal;
                        player.hang_cooldown = 0.25;
                        player.velocity.y = -2.0;
                    } else if keys.just_pressed(KeyCode::Space) {
                        // Hop off.
                        player.move_mode = PlayerMoveMode::Normal;
                        player.hang_cooldown = 0.25;
                        player.velocity.y = tuning.jump_speed * 0.7;
                    } else {
                        // Crawl along the underside.
                        apply_accel_friction(
                            &mut player.velocity,
                            horiz,
                            tuning.crouch_speed * 0.9,
                            tuning.ground_accel * 0.8,
                            tuning.ground_friction * 0.4,
                            tuning.stop_speed,
                            dt,
                        );
                        player.velocity.y = 0.0;
                        transform.translation.y = bottom + he.y;
                    }
                }
                None => {
                    // Surface ended or moved out of reach: drop.
                    player.move_mode = PlayerMoveMode::Normal;
                    player.hang_cooldown = 0.3;
                    player.velocity.y = -4.0;
                }
            }
        } else if hang_key
            && player.hang_cooldown <= 0.0
            && !player.on_ground
            && !underwater
            && !player.slamming
            && player.launch <= 0.0
            && player.velocity.y <= 2.0
            && let Some((_, bottom)) = find_hang_surface(&level, transform.translation, he)
        {
            player.move_mode = PlayerMoveMode::Hanging;
            player.velocity = Vec3::ZERO;
            transform.translation.y = bottom + he.y;
        }
        let hanging = player.move_mode == PlayerMoveMode::Hanging;

        // Formal floor normal (sampled surface gradient). An entity wedge
        // underfoot wins over the block sample so slope slide / steepness
        // behaves identically for both.
        if player.on_ground {
            let wedge_n = solids.floor_normal(transform.translation.x, transform.translation.z);
            let mut sampled_n =
                floor_normal_at(&level, transform.translation.x, transform.translation.z);
            if wedge_n != Vec3::Y {
                sampled_n = wedge_n;
            }
            if sampled_n.length_squared() > 0.0 {
                move_state.floor_normal = sampled_n;
            }
        }

        let slope = if player.on_ground {
            slope_slide(&level, transform.translation, he)
        } else {
            None
        };
        // Prefer normal-based steepness when we have a good sample.
        let steep = player.on_ground && move_state.floor_normal.y < tuning.walkable_normal_y;
        let mut sliding = slope.is_some() || steep;

        let launch_locked = player.launch > 0.0;

        let ground_kind = if player.on_ground {
            ground_block(&level, transform.translation.x, transform.translation.z)
        } else {
            None
        };
        let on_ice = ground_kind.is_some_and(|k| k == BlockKind::Ice);

        // Horizontal control: accel/friction (not instant wish velocity).
        // While hanging, the crawl accel already ran; skip normal control.
        let steering = horiz.length_squared() > 1e-4;
        if !hanging && player.slamming {
            player.velocity.x = 0.0;
            player.velocity.z = 0.0;
        } else if !hanging && sliding && !steering && !launch_locked && !underwater {
            if let Some(dir) = slope {
                player.velocity.x = dir.x * tuning.slide_speed;
                player.velocity.z = dir.y * tuning.slide_speed;
            } else {
                // Steep normal but no discrete slope dir: slide along -floor_xz.
                let mut d = Vec3::new(move_state.floor_normal.x, 0.0, move_state.floor_normal.z);
                if d.length_squared() > 1e-6 {
                    d = d.normalize();
                    player.velocity.x = d.x * tuning.slide_speed;
                    player.velocity.z = d.z * tuning.slide_speed;
                } else {
                    sliding = false;
                }
            }
        } else if !hanging && !launch_locked {
            let (accel, friction, max_speed) = if underwater {
                (tuning.swim_accel, tuning.swim_friction, tuning.swim_speed)
            } else if player.on_ground {
                (
                    tuning.ground_accel,
                    tuning.ground_friction,
                    if effectively_crouching {
                        tuning.crouch_speed
                    } else {
                        tuning.move_speed
                    },
                )
            } else {
                (tuning.air_accel, tuning.air_friction, tuning.move_speed)
            };
            let (accel, friction) = if on_ice {
                // Ice: keep momentum, weak control.
                (accel * 0.45, friction * 0.06)
            } else {
                (accel, friction)
            };
            let speed_mul = if player.speed_boost > 0.0 { 1.55 } else { 1.0 };
            apply_accel_friction(
                &mut player.velocity,
                horiz,
                max_speed * speed_mul,
                accel,
                friction,
                tuning.stop_speed,
                dt,
            );
        } else if !hanging {
            // Light air steer while launched.
            apply_accel_friction(
                &mut player.velocity,
                horiz,
                tuning.move_speed * 0.85,
                tuning.air_accel * 0.65,
                0.0,
                tuning.stop_speed,
                dt,
            );
        }
        if effectively_crouching && !underwater {
            let max_c = tuning.crouch_speed * if player.speed_boost > 0.0 { 1.55 } else { 1.0 };
            let hs = Vec2::new(player.velocity.x, player.velocity.z).length();
            if hs > max_c && hs > 1e-6 {
                let s = max_c / hs;
                player.velocity.x *= s;
                player.velocity.z *= s;
            }
        }

        move_state.sliding = sliding;

        player.coyote = (player.coyote - dt).max(0.0);
        player.jump_buffer = (player.jump_buffer - dt).max(0.0);
        player.launch = (player.launch - dt).max(0.0);
        player.speed_boost = (player.speed_boost - dt).max(0.0);
        player.invuln = (player.invuln - dt).max(0.0);

        if keys.just_pressed(KeyCode::Space) {
            player.jump_buffer = tuning.jump_buffer;
        }
        if underwater {
            if keys.pressed(KeyCode::Space) {
                player.velocity.y = tuning.swim_speed;
            }
        } else if player.jump_buffer > 0.0 && player.coyote > 0.0 {
            player.velocity.y = if sliding {
                tuning.slide_jump_speed
            } else {
                tuning.jump_speed
            };
            player.jump_buffer = 0.0;
            player.coyote = 0.0;
            player.on_ground = false;
        }

        if !underwater
            && crouch_pressed
            && !player.on_ground
            && !hanging
            && player.velocity.y <= 2.0
        {
            player.velocity.y = tuning.slam_speed;
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
                player.launch = tuning.launch_lock;
                player.on_ground = false;
                player.coyote = 0.0;
                player.slamming = false;
                ScreenEffects::add_trauma(&mut trauma, 0.2);
            }
        }

        // Conveyor: push the player along the block's facing while on top.
        if !hanging
            && let Some(kind) = ground_kind
            && kind.is_conveyor()
            && kind.conveyor_active(onoff.on)
            && let Some(rot) =
                ground_block_rot(&level, transform.translation.x, transform.translation.z)
        {
            let dir = Quat::from_rotation_y(rot as f32 * std::f32::consts::FRAC_PI_2) * Vec3::X;
            player.velocity += dir * 6.0 * dt;
        }

        let gravity = if underwater {
            tuning.water_gravity
        } else {
            tuning.gravity
        };
        let max_fall = if underwater {
            tuning.max_fall_water
        } else {
            tuning.max_fall
        };
        if !hanging {
            player.velocity.y = (player.velocity.y + gravity * dt).max(max_fall);
        }

        // Climb surface: hold W while overlapping a Climb block to ascend.
        let on_climb = overlaps_kind(transform.translation, he, &level, BlockKind::Climb);
        if on_climb && kb && !hanging && keys.pressed(KeyCode::KeyW) {
            player.velocity.y = 4.5;
        }

        // Stay glued to the ground.
        let grounding_ok = (player.was_on_ground || player.on_ground)
            && !underwater
            && !hanging
            && player.velocity.y <= 0.0;
        if grounding_ok {
            let top = ground_height(&level, transform.translation.x, transform.translation.z);
            let feet = transform.translation.y - move_he.y;
            // Only stick to the ground while it's actually near the feet.
            if top.is_finite() && (top - feet).abs() <= GROUND_STEP_MAX {
                transform.translation.y = top + move_he.y;
            }
        }

        let result = move_and_collide(
            transform.translation,
            move_he,
            player.velocity * dt,
            &level,
            &solids.solids,
        );
        if result.hit_x || result.hit_z {
            let n = result.wall_normal;
            if n.length_squared() > 1e-6 {
                let hx = player.velocity.x;
                let hz = player.velocity.z;
                let speed_before = (hx * hx + hz * hz).sqrt();
                let into = hx * n.x + hz * n.z; // < 0 = moving into wall
                if into < -1e-5 {
                    player.velocity.x -= n.x * into;
                    player.velocity.z -= n.z * into;
                    // Preserve along-wall speed so grazing doesn't feel like mud.
                    let tx = player.velocity.x;
                    let tz = player.velocity.z;
                    let tlen = (tx * tx + tz * tz).sqrt();
                    if tlen > 1e-4 && speed_before > tlen {
                        let s = speed_before / tlen;
                        player.velocity.x = tx * s;
                        player.velocity.z = tz * s;
                    }
                }
            } else {
                if result.hit_x {
                    player.velocity.x = 0.0;
                }
                if result.hit_z {
                    player.velocity.z = 0.0;
                }
            }
        }
        if result.hit_y || result.stepped_up {
            if player.velocity.y < 0.0 || result.stepped_up {
                player.velocity.y = 0.0;
            } else if result.hit_y && player.velocity.y > 0.0 {
                player.velocity.y = 0.0; // ceiling
            }
        }
        if result.wall_normal != Vec3::ZERO {
            move_state.wall_normal = result.wall_normal;
        }
        if result.on_ground {
            move_state.floor_normal = result.floor_normal;
        }

        // One-way plate riding: probe the plate top under our feet after the
        // move and carry the player with the plate's motion.
        let mut pos = result.pos;
        let feet_y = pos.y - move_he.y;
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
                pos.y = top + move_he.y;
                player.velocity.y = 0.0;
                on_plate = true;
                move_state.floor_normal = Vec3::Y;
                break;
            }
        }
        let grounded_now = result.on_ground
            || on_plate
            || (!hanging && player.was_on_ground && player.velocity.y <= 0.0);
        if grounded_now && !on_plate && player.velocity.y <= 0.0 {
            let top = ground_height(&level, pos.x, pos.z);
            if top.is_finite() {
                let eff_he = move_he.y;
                let feet = pos.y - eff_he;
                if feet < top + 0.001 && top <= feet + GROUND_STEP_MAX {
                    pos.y = top + eff_he;
                }
            }
        }
        transform.translation = pos;

        player.grip_cooldown = (player.grip_cooldown - dt).max(0.0);
        if player.gripping {
            if keys.just_pressed(KeyCode::Space) || keys.pressed(KeyCode::KeyW) {
                transform.translation = player.grip_mantle;
                player.gripping = false;
                player.grip_top = 0.0;
                player.velocity = Vec3::ZERO;
                player.coyote = tuning.coyote_time;
                player.grip_cooldown = 0.15;
            } else if keys.pressed(KeyCode::KeyS) {
                player.gripping = false;
                player.grip_top = 0.0;
                player.velocity = Vec3::ZERO;
                player.grip_top = 0.0;
                player.grip_cooldown = 0.35;
            } else {
                let wt = ground_height(&level, player.grip_anchor.x, player.grip_anchor.z);
                let above = IVec3::new(
                    player.grip_anchor.x.floor() as i32,
                    (player.grip_anchor.y + 1.0).floor() as i32,
                    player.grip_anchor.z.floor() as i32,
                );
                let lip_ok =
                    wt.is_finite() && (wt - player.grip_top).abs() < 0.55 && !level.is_solid(above);
                if !lip_ok {
                    player.gripping = false;
                    player.grip_top = 0.0;
                    player.grip_cooldown = 0.2;
                } else {
                    transform.translation.y = player.grip_top - he.y + 0.14;
                    player.velocity = Vec3::ZERO;
                }
            }
        } else if false
            && player.grip_cooldown <= 0.0
            && !result.on_ground
            && player.velocity.y <= 0.5
            && !underwater
            && !player.slamming
            && player.launch <= 0.0
        {
            // Ledge grab is disabled: leads to buggy interactions with most
            // blocks. The rally is left in so it can be re-enabled cleanly (far-future).
            if let Some(g) = ledge_grip(
                &level,
                transform.translation,
                he,
                player.velocity,
                result.hit_x,
                result.hit_z,
            ) {
                transform.translation = g.hang_pos;
                player.gripping = true;
                player.grip_top = g.wall_top;
                player.grip_mantle = g.mantle_pos;
                player.grip_anchor = Vec3::new(g.hang_pos.x, g.wall_top, g.hang_pos.z);
                player.grip_time = 0.0;
                player.velocity = Vec3::ZERO;
            }
        }

        let was_grounded = result.on_ground || on_plate || player.gripping;
        if was_grounded && !player.was_on_ground {
            let impact = (-player.fall_speed).max(0.0);
            if player.slamming {
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
            player.coyote = tuning.coyote_time;
            player.launch = 0.0;
        } else {
            player.on_ground = false;
        }

        move_state.grounded = player.on_ground;
        move_state.action = if underwater {
            ActionState::Swim
        } else if player.slamming {
            ActionState::Slam
        } else if player.launch > 0.0 {
            ActionState::Launch
        } else if !player.on_ground {
            ActionState::Air
        } else {
            ActionState::Run
        };

        if player.on_ground
            && player.velocity.y <= 0.0
            && ground_block(&level, transform.translation.x, transform.translation.z)
                == Some(BlockKind::Bounce)
        {
            player.velocity.y = JUMP_SPEED * 1.35;
            player.on_ground = false;
            player.coyote = 0.0;
            player.was_on_ground = false;
        }
    }
}

pub fn sync_mode(
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    mut q: Query<(&mut Transform, &mut Player, &mut MoveState, &mut Visibility)>,
) {
    if !mode.is_changed() {
        return;
    }
    for (mut transform, mut player, mut move_state, mut vis) in &mut q {
        match *mode {
            MakerMode::Play => reset_player_run(
                &mut transform,
                &mut player,
                &mut move_state,
                &mut vis,
                &level,
            ),
            MakerMode::Edit => {
                *vis = Visibility::Hidden;
            }
        }
    }
}
