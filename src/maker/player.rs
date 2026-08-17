use std::collections::HashMap;

use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

use super::MakerCleanup;
use super::block::BlockKind;
use super::camera::CameraRig;
use super::collision::{
    floor_normal_at, ground_height, ledge_grip, move_and_collide_ex, overlaps_kind, slope_slide,
    stand_headroom, support_height_footprint,
};
use super::entities_runtime::{DriftPlate, ModelAnim, ModelMaterial, RuntimeSolids};
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

/// Small gap kept between the head and the underside while hanging, so the
/// body hangs just below the slab instead of embedding into it.
const HANG_SKIN: f32 = 0.02;

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
    /// Release jump while rising: multiply upward vel (variable height).
    pub jump_cut_mult: f32,
    /// Max fall speed while sliding down a wall (airborne + into wall).
    pub wall_slide_max_fall: f32,
    /// Horizontal impulse away from wall on wall-jump.
    pub wall_jump_push: f32,
    /// Upward speed on wall-jump.
    pub wall_jump_up: f32,
    /// Seconds after wall-jump before another wall grab/jump.
    pub wall_jump_lock: f32,
    /// Wall slide + wall jump while airborne against a wall. Disabled by
    /// default (jank-heavy).
    pub allow_wall_kick: bool,
    /// Ledge grab / mantle (cling to a boxy lip, pull up). Disabled by
    /// default.
    pub allow_ledge_grab: bool,
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
            jump_cut_mult: 0.45,
            wall_slide_max_fall: -6.5,
            wall_jump_push: 7.5,
            wall_jump_up: 8.5,
            wall_jump_lock: 0.18,
            allow_wall_kick: false,
            allow_ledge_grab: false,
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
    /// Position at the start of this frame's collide step (for top-contact
    /// contacts that need the pre-move feet height).
    pub pre_move_pos: Vec3,
    /// Brief lock after wall-jump so we don't re-stick the same frame.
    pub wall_lock: f32,
    /// True between jump start and the first jump cut / reaching the apex:
    /// marks the jump as player-initiated so only it gets the variable
    /// jump cut (bounces and launches keep full height).
    pub jump_held: bool,
    /// Cooldown so bounce pads fire once per landing, not every frame.
    pub bounce_cd: f32,
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
            pre_move_pos: Vec3::ZERO,
            wall_lock: 0.0,
            jump_held: false,
            bounce_cd: 0.0,
        }
    }
}

pub fn spawn_center(level: &LevelDocument) -> Vec3 {
    let s = level.data.spawn;
    Vec3::new(s[0] as f32 + 0.5, s[1] as f32 + 0.9, s[2] as f32 + 0.5)
}

/// The block kind + yaw of the surface the player is *actually standing on*:
/// the topmost solid surface under `(wx, wz)` must be within one step of the
/// feet, otherwise a tall neighbor column / thin overlayer is ignored.
fn ground_surface_block(
    level: &LevelDocument,
    wx: f32,
    wz: f32,
    feet_y: f32,
) -> Option<(BlockKind, u8)> {
    let h = ground_height(level, wx, wz);
    if !h.is_finite() {
        return None;
    }
    // Must be the surface we're actually standing on.
    if (h - feet_y).abs() > GROUND_STEP_MAX + 0.05 {
        return None;
    }
    let cell = IVec3::new(
        wx.floor() as i32,
        (h - 0.001).floor() as i32,
        wz.floor() as i32,
    );
    let b = level.get_block(cell)?;
    if !level.kind_is_solid(b.kind) {
        return None;
    }
    // Empty half of a V-slab / V-slope etc. is not a floor under this point.
    if super::collision::surface_top_height_opt(b, wx, wz).is_none() {
        return None;
    }
    Some((b.kind, b.rot))
}

/// A hangable underside directly above the player's head, if any. Returns the
/// cell and the world height of the slab's bottom surface.
fn find_hang_surface(level: &LevelDocument, pos: Vec3, he: Vec3) -> Option<(IVec3, f32)> {
    let head = pos.y + he.y;
    // Sample the head cell plus four inset corner columns so rails/conveyors
    // keep the grab when the body straddles a cell corner.
    let samples = [
        (pos.x, pos.z),
        (pos.x + he.x * 0.6, pos.z),
        (pos.x - he.x * 0.6, pos.z),
        (pos.x, pos.z + he.z * 0.6),
        (pos.x, pos.z - he.z * 0.6),
    ];
    let top = head.floor() as i32;
    let mut best: Option<(IVec3, f32, f32)> = None;
    for (sx, sz) in samples {
        let cx = sx.floor() as i32;
        let cz = sz.floor() as i32;
        // Check the column the head is in and one above (in case the head just
        // crossed a cell boundary), and one below for the gap.
        for y in (top - 1..=top + 1).rev() {
            let cell = IVec3::new(cx, y, cz);
            if let Some(b) = level.get_block(cell)
                && level.kind_is_solid(b.kind)
                && b.kind.has_hangable_underside()
                && let Some(bottom) = super::collision::surface_bottom_height(b, sx, sz)
            {
                // Only grab while the head is roughly against the underside.
                let err = (bottom - head).abs();
                if err <= 0.35 && best.map_or(true, |(_, _, e)| err < e) {
                    best = Some((cell, bottom, err));
                }
            }
        }
    }
    best.map(|(c, b, _)| (c, b))
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
    player.move_mode = PlayerMoveMode::Normal;
    player.hang_cooldown = 0.0;
    player.pre_move_pos = spawn;
    player.wall_lock = 0.0;
    player.jump_held = false;
    player.bounce_cd = 0.0;
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

fn pad_pressed(gamepads: &Query<&Gamepad>, btn: GamepadButton) -> bool {
    gamepads.iter().any(|g| g.pressed(btn))
}

fn pad_just_pressed(gamepads: &Query<&Gamepad>, btn: GamepadButton) -> bool {
    gamepads.iter().any(|g| g.just_pressed(btn))
}

/// Combined keyboard + gamepad move wish (camera-relative, applied by caller).
fn read_move_wish(keys: &ButtonInput<KeyCode>, kb_ok: bool, gamepads: &Query<&Gamepad>) -> Vec2 {
    let mut wish = Vec2::ZERO;
    if kb_ok {
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
    for pad in gamepads {
        let v = pad.left_stick();
        if v.length() > 0.2 {
            wish += v;
        }
    }
    if wish.length_squared() > 1.0 {
        wish = wish.normalize();
    }
    wish
}

/// Single source of truth for Play-mode buttons. Keyboard respects input
/// capture (dialogs / overlays); gamepad is always live while playing.
#[derive(Clone, Copy, Debug)]
pub struct PlayInput {
    /// Camera-relative move wish (2D: x = right, y = forward).
    pub wish: Vec2,
    pub jump_pressed: bool,
    pub jump_down: bool,
    pub crouch_down: bool,
    pub crouch_tapped: bool,
    pub hang_down: bool,
    pub interact_pressed: bool,
    pub throw_pressed: bool,
    pub reset_pressed: bool,
    /// Crouch + back (used to drop through one-way platforms).
    pub drop_through: bool,
}

fn keyboard(keys: &ButtonInput<KeyCode>, kb_ok: bool, key: KeyCode) -> bool {
    kb_ok && keys.pressed(key)
}

/// Combine keyboard + gamepad into one Play-mode input snapshot. All systems
/// that read Play controls must use this (or a field of it) so input can never
/// drift between keyboard and pad paths.
pub fn read_play_input(
    keys: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    kb_ok: bool,
) -> PlayInput {
    let wish = read_move_wish(keys, kb_ok, gamepads);
    let crouch_down =
        keyboard(keys, kb_ok, KeyCode::ShiftLeft) || pad_pressed(gamepads, GamepadButton::East);
    let back = wish.y < -0.5
        || keyboard(keys, kb_ok, KeyCode::KeyS)
        || gamepads.iter().any(|g| g.dpad().y < -0.5);
    PlayInput {
        wish,
        jump_pressed: keyboard(keys, kb_ok, KeyCode::Space)
            || pad_just_pressed(gamepads, GamepadButton::South),
        jump_down: keyboard(keys, kb_ok, KeyCode::Space)
            || pad_pressed(gamepads, GamepadButton::South),
        crouch_down,
        crouch_tapped: keyboard(keys, kb_ok, KeyCode::ShiftLeft)
            || pad_just_pressed(gamepads, GamepadButton::East),
        hang_down: keyboard(keys, kb_ok, KeyCode::KeyE)
            || pad_pressed(gamepads, GamepadButton::West),
        interact_pressed: keyboard(keys, kb_ok, KeyCode::KeyI)
            || pad_just_pressed(gamepads, GamepadButton::North),
        throw_pressed: keyboard(keys, kb_ok, KeyCode::KeyF)
            || pad_just_pressed(gamepads, GamepadButton::RightTrigger),
        reset_pressed: keyboard(keys, kb_ok, KeyCode::KeyR)
            || pad_just_pressed(gamepads, GamepadButton::Select),
        drop_through: crouch_down && back,
    }
}

pub fn player_controller(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    capture: Res<InputCapture>,
    level: Res<LevelDocument>,
    rig: Res<CameraRig>,
    solids: Res<RuntimeSolids>,
    tuning: Res<MoveTuning>,
    mut commands: Commands,
    mut trauma: ResMut<Trauma>,
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
        let input = read_play_input(&keys, &gamepads, kb);
        let wish = input.wish;

        let (sin, cos) = rig.yaw.sin_cos();
        let forward = Vec3::new(-sin, 0.0, -cos);
        let right = Vec3::new(cos, 0.0, -sin);
        let mut horiz = forward * wish.y + right * wish.x;
        if horiz.length_squared() > 1.0 {
            horiz = horiz.normalize();
        }
        move_state.wish_dir = horiz;

        player.wall_lock = (player.wall_lock - dt).max(0.0);

        let want_crouch = input.crouch_down;
        let crouch_pressed = input.crouch_tapped;
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

        // Hanging: hold E (or West) under a hangable underside (thin conveyor /
        // hang rail). Overrides gravity while active.
        let hang_key = input.hang_down;
        player.hang_cooldown = (player.hang_cooldown - dt).max(0.0);
        if player.move_mode == PlayerMoveMode::Hanging {
            match find_hang_surface(&level, transform.translation, move_he) {
                Some((_, bottom)) => {
                    if !hang_key || underwater {
                        player.move_mode = PlayerMoveMode::Normal;
                        player.hang_cooldown = 0.25;
                        player.velocity.y = -2.0;
                    } else if input.jump_pressed {
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
                        transform.translation.y = bottom - move_he.y - HANG_SKIN;
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
            && let Some((_, bottom)) = find_hang_surface(&level, transform.translation, move_he)
        {
            player.move_mode = PlayerMoveMode::Hanging;
            player.velocity = Vec3::ZERO;
            transform.translation.y = bottom - move_he.y - HANG_SKIN;
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

        // Crouch drop-through: hold crouch + S (or crouch + DPad-Down) while
        // grounded to fall through one-way platforms (waived in the collider
        // this frame). Computed before ground sampling so bounce/ice/conveyor
        // surface reads also ignore one-ways during the drop.
        let drop_through = effectively_crouching
            && input.drop_through
            && player.on_ground
            && !hanging
            && !underwater
            && player.velocity.y <= 0.0;

        let feet_now = transform.translation.y - move_he.y;
        let ground_surf = if player.on_ground && !drop_through {
            ground_surface_block(
                &level,
                transform.translation.x,
                transform.translation.z,
                feet_now,
            )
        } else {
            None
        };
        let ground_kind = ground_surf.map(|(k, _)| k);
        let on_ice = ground_kind == Some(BlockKind::Ice);

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

        if input.jump_pressed {
            player.jump_buffer = tuning.jump_buffer;
        }
        if underwater {
            if input.jump_down {
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
            player.jump_held = true;
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

        // Conveyor: push the player toward the belt velocity while on top.
        // Blends instead of accumulating so ice/conveyor stacks don't fight.
        if !hanging
            && let Some((kind, rot)) = ground_surf
            && kind.is_conveyor()
            && kind.conveyor_active(onoff.on)
        {
            let dir = Quat::from_rotation_y(rot as f32 * std::f32::consts::FRAC_PI_2) * Vec3::X;
            let belt = dir * 4.0;
            let t = 1.0 - (-8.0 * dt).exp();
            player.velocity.x += (belt.x - player.velocity.x) * t;
            player.velocity.z += (belt.z - player.velocity.z) * t;
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

        if !underwater
            && !hanging
            && !player.slamming
            && player.launch <= 0.0
            && player.velocity.y > 0.0
            && player.jump_held
            && !input.jump_down
        {
            if player.velocity.y > tuning.jump_speed * 0.15 {
                player.velocity.y *= tuning.jump_cut_mult;
            }
            player.jump_held = false;
        }
        if player.on_ground || player.velocity.y <= 0.0 {
            player.jump_held = false;
        }

        // Project velocity out of the floor on slopes (grounded, falling into
        // the ramp): stable walking without the slope "kick".
        if player.on_ground
            && !hanging
            && !player.slamming
            && !underwater
            && player.velocity.y <= 0.0
        {
            let n = move_state.floor_normal;
            if n.y > 0.05 && n.y < 0.999 {
                let into = player.velocity.dot(n);
                if into < 0.0 {
                    player.velocity -= n * into;
                }
            }
        }

        // Climb surface: forward (W / stick-up) ascends, back slides down slowly,
        // anything else clings so you don't instantly peel off the wall like others.
        let on_climb = overlaps_kind(transform.translation, move_he, &level, BlockKind::Climb);
        if on_climb && !hanging && !player.slamming {
            if input.wish.y > 0.35 {
                player.velocity.y = 4.5;
            } else if input.wish.y < -0.35 {
                player.velocity.y = player.velocity.y.min(-2.5).max(tuning.wall_slide_max_fall);
            } else if !player.on_ground {
                player.velocity.y = player.velocity.y.max(-1.5);
            }
        }

        // Stay glued to the ground (blocks + runtime solids, Warbell-style).
        let grounding_ok = (player.was_on_ground || player.on_ground)
            && !underwater
            && !hanging
            && !drop_through
            && player.velocity.y <= 0.0;
        if grounding_ok {
            let top =
                support_height_footprint(&level, &solids.solids, transform.translation, move_he);
            let feet = transform.translation.y - move_he.y;
            // Only stick to the ground while it's actually near the feet.
            if top.is_finite() && (top - feet).abs() <= GROUND_STEP_MAX {
                transform.translation.y = top + move_he.y;
            }
        }
        if drop_through {
            // Nudge down so the feet clear the one-way band this frame.
            player.velocity.y = player.velocity.y.min(-2.0);
            player.on_ground = false;
            player.was_on_ground = false;
            player.coyote = 0.0;
        }

        // Record the pose before moving: top-contact interactions (pads,
        // crumble plates) use it to detect a real crossing, not the
        // post-collision pose where vertical velocity has already been zeroed.
        player.pre_move_pos = transform.translation;

        let result = move_and_collide_ex(
            transform.translation,
            move_he,
            player.velocity * dt,
            &level,
            &solids.solids,
            drop_through,
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

        let into_wall = result.wall_normal.length_squared() > 1e-6 && {
            let n = result.wall_normal;
            horiz.dot(n) < -0.45
                && Vec3::new(player.velocity.x, 0.0, player.velocity.z).dot(n) < -0.05
                && (move_state.floor_normal.y > 0.99 || !player.on_ground)
        };
        if tuning.allow_wall_kick
            && !underwater
            && !hanging
            && !player.on_ground
            && !player.slamming
            && player.launch <= 0.0
            && player.wall_lock <= 0.0
            && player.velocity.y < 0.0
            && into_wall
            && (result.hit_x || result.hit_z)
        {
            if player.velocity.y < tuning.wall_slide_max_fall {
                player.velocity.y = tuning.wall_slide_max_fall;
            }
            if player.jump_buffer > 0.0 {
                let n = result.wall_normal.normalize_or_zero();
                player.velocity.x = n.x * tuning.wall_jump_push;
                player.velocity.z = n.z * tuning.wall_jump_push;
                player.velocity.y = tuning.wall_jump_up;
                player.jump_buffer = 0.0;
                player.coyote = 0.0;
                player.wall_lock = tuning.wall_jump_lock;
                player.on_ground = false;
                player.jump_held = true;
                move_state.wall_normal = n;
            }
        }

        // One-way plate riding: probe the plate top under our feet after the
        // move and carry the player with the plate's motion.
        let mut pos = result.pos;
        let feet_y = pos.y - move_he.y;
        let prev_feet = player.pre_move_pos.y - move_he.y;
        let mut on_plate = false;
        for (dtf, drift) in &plates {
            let top = dtf.translation.y + 0.12;
            let over = (pos.x - dtf.translation.x).abs() < 0.7 + move_he.x
                && (pos.z - dtf.translation.z).abs() < 0.7 + move_he.z;
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
        let grounded_now = result.on_ground || on_plate;
        if grounded_now && !on_plate && player.velocity.y <= 0.0 {
            let top = support_height_footprint(&level, &solids.solids, pos, move_he);
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
        if player.gripping && !tuning.allow_ledge_grab {
            // Feature toggled off mid-grip: drop cleanly.
            player.gripping = false;
            player.grip_top = 0.0;
            player.velocity = Vec3::ZERO;
        }
        if player.gripping {
            if input.jump_pressed || keys.pressed(KeyCode::KeyW) {
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
        } else if tuning.allow_ledge_grab
            && player.grip_cooldown <= 0.0
            && player.wall_lock <= 0.0
            && !result.on_ground
            && player.velocity.y <= 0.5
            && !underwater
            && !player.slamming
            && player.launch <= 0.0
            && !hanging
            && !on_climb
            && result.wall_normal.y.abs() < 0.1
            && (result.hit_x || result.hit_z)
        {
            // Ledge mantle: only when clearly into a wall and lip is boxy.
            // Skips slopes/thin/hang-rails via is_grabbable_lip.
            if let Some(g) = ledge_grip(
                &level,
                transform.translation,
                he, // standing half-extents for mantle clearance
                player.velocity,
                result.hit_x,
                result.hit_z,
            ) {
                // Extra: require approach mostly into the face.
                let face3 = Vec3::new(g.face.x, 0.0, g.face.y);
                let approach = Vec3::new(player.velocity.x, 0.0, player.velocity.z);
                let into = approach.dot(face3);
                if into > 0.4 || horiz.dot(face3) > 0.35 {
                    transform.translation = g.hang_pos;
                    player.gripping = true;
                    player.grip_top = g.wall_top;
                    player.grip_mantle = g.mantle_pos;
                    player.grip_anchor = Vec3::new(g.hang_pos.x, g.wall_top, g.hang_pos.z);
                    player.grip_time = 0.0;
                    player.velocity = Vec3::ZERO;
                    player.slamming = false;
                }
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

        if player.on_ground
            && !sliding
            && !underwater
            && !hanging
            && move_state.floor_normal.y > 0.2
            && move_state.floor_normal.y < 0.999
        {
            let n = move_state.floor_normal.normalize_or_zero();
            let v = player.velocity;
            // Keep only the component tangent to the floor.
            let into = v.dot(n);
            if into < 0.0 {
                player.velocity = v - n * into;
            }
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

        player.bounce_cd = (player.bounce_cd - dt).max(0.0);
        if player.on_ground
            && player.velocity.y <= 0.0
            && player.bounce_cd <= 0.0
            && !hanging
            && let Some((BlockKind::Bounce, _)) = ground_surface_block(
                &level,
                transform.translation.x,
                transform.translation.z,
                transform.translation.y - move_he.y,
            )
        {
            player.velocity.y = tuning.jump_speed * 1.35;
            player.on_ground = false;
            player.coyote = 0.0;
            player.was_on_ground = false;
            player.bounce_cd = 0.2;
            player.jump_held = false; // full bounce, not cuttable
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::maker::block::BlockShape;
    use crate::maker::level::BlockData;

    fn level_with(blocks: &[(IVec3, BlockKind, BlockShape)]) -> LevelDocument {
        let mut level = LevelDocument::default();
        for (pos, kind, shape) in blocks {
            level.set_block(
                *pos,
                Some(BlockData {
                    position: pos.to_array(),
                    kind: *kind,
                    shape: *shape,
                    rot: 0,
                    waterlogged: false,
                }),
            );
        }
        level
    }

    #[test]
    fn ground_surface_block_ignores_tall_neighbor_column() {
        // Stone pillar rising above the grass floor in the same column: the
        // topmost surface is the pillar top, not the floor we're standing on.
        let level = level_with(&[
            (IVec3::new(0, 1, 0), BlockKind::Grass, BlockShape::Full),
            (IVec3::new(0, 2, 0), BlockKind::Stone, BlockShape::Full),
        ]);
        // Feet on the grass floor (top 2.0): must NOT read the pillar top (3.0).
        assert_eq!(ground_surface_block(&level, 0.5, 0.5, 2.0), None);
        // Feet on the pillar are accepted.
        assert_eq!(
            ground_surface_block(&level, 0.5, 0.5, 3.0),
            Some((BlockKind::Stone, 0))
        );
        // Plain grass-only level: the floor itself wins.
        let level = level_with(&[(IVec3::new(0, 1, 0), BlockKind::Grass, BlockShape::Full)]);
        assert_eq!(
            ground_surface_block(&level, 0.5, 0.5, 2.0),
            Some((BlockKind::Grass, 0))
        );
    }

    #[test]
    fn ground_surface_block_requires_feet_near_the_surface() {
        // Bounce pad on a tall pedestal: standing next to it (feet 1.0 on the
        // boundary floor) must not fire as if standing on the pad.
        let level = level_with(&[(IVec3::new(0, 2, 0), BlockKind::Bounce, BlockShape::Full)]);
        assert_eq!(ground_surface_block(&level, 0.5, 0.5, 1.0), None);
        // On top of the pad (feet at 3.0) it is the walk surface.
        assert_eq!(
            ground_surface_block(&level, 0.5, 0.5, 3.0),
            Some((BlockKind::Bounce, 0))
        );
        // Too far below the surface (fell into a gap) is not "standing" either.
        assert_eq!(ground_surface_block(&level, 0.5, 0.5, 0.3), None);
    }

    #[test]
    fn ground_surface_block_empty_slab_half_is_not_floor() {
        // V-slab against local -Z: the +Z half of the cell has no material, so
        // it must not count as a walk surface at that sample point.
        let level = level_with(&[(
            IVec3::new(0, 1, 0),
            BlockKind::Stone,
            BlockShape::VerticalSlab,
        )]);
        assert_eq!(ground_surface_block(&level, 0.5, 0.75, 2.0), None);
        assert_eq!(
            ground_surface_block(&level, 0.5, 0.25, 2.0),
            Some((BlockKind::Stone, 0))
        );
    }
}
