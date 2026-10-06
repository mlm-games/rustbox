//! Animated character presentation: the play-mode player model, with the clip
//! selection and facing rules the original ran in
//! `entities_runtime::tick_model_anims`. The player transform carries no
//! facing, so the model yaw is smoothed here at the original's turn rate.

use std::cell::Cell;
use std::rc::Rc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use glam::{Mat4, Quat, Vec2, Vec3, Vec3Swizzles};
use repame_view3d::Frame3d;

use super::assets::{CharacterModel, ModelAssets, Playback};
use super::player::{ActionState, MoveState, Player, PlayerTransform};

/// Root placement the original gave the player model
/// (`translation(0, -0.9) * scale(0.6)` under the player transform).
const PLAYER_OFFSET: Vec3 = Vec3::new(0.0, -0.9, 0.0);
const PLAYER_SCALE: f32 = 0.6;

/// Turn rate for the smoothed model yaw (the original's exponential slerp).
const TURN_RATE: f32 = 14.0;

/// Player clips, as named in `Character_Male_2.gltf`.
const PLAYER_IDLE: &str = "Idle";
const PLAYER_RUN: &str = "Run";
const PLAYER_AIR: &str = "Jump";

/// One loaded model plus its playback clock.
struct Playing {
    model: Rc<CharacterModel>,
    playback: Playback,
}

impl Playing {
    fn new(model: Rc<CharacterModel>) -> Self {
        let playback = model.playback();
        Self { model, playback }
    }
}

pub struct CharacterView {
    player: Option<Playing>,
    player_yaw: Cell<f32>,
}

impl Default for CharacterView {
    fn default() -> Self {
        Self::new()
    }
}

impl CharacterView {
    pub fn new() -> Self {
        Self {
            player: None,
            player_yaw: Cell::new(0.0),
        }
    }

    /// Which clip the player should be playing, from its action state and
    /// horizontal speed.
    fn player_clip(player: &Player, state: Option<&MoveState>) -> &'static str {
        let action = state.map(|s| s.action).unwrap_or(ActionState::Run);
        let airborne = matches!(
            action,
            ActionState::Air | ActionState::Slam | ActionState::Launch
        ) || !player.on_ground;
        let horizontal = player.velocity.xz().length();
        if action == ActionState::Swim {
            return if horizontal > 0.6 {
                PLAYER_RUN
            } else {
                PLAYER_IDLE
            };
        }
        if airborne {
            return PLAYER_AIR;
        }
        let wish = state
            .map(|s| s.wish_dir.xz().length_squared())
            .unwrap_or(0.0);
        if horizontal > 1.0 || wish > 0.25 {
            PLAYER_RUN
        } else {
            PLAYER_IDLE
        }
    }

    /// Draw the player for this frame. `player_entity` is the app's player
    /// root. Returns false when no player model is available, so
    /// the caller can keep its placeholder geometry.
    pub fn draw(
        &mut self,
        player_entity: Entity,
        world: &World,
        assets: &mut ModelAssets,
        dt: f32,
        frame: &mut Frame3d,
    ) -> bool {
        if self.player.is_none() {
            self.player = assets.player().map(Playing::new);
        }
        let mut player_modelled = false;
        if let Some(playing) = self.player.as_mut() {
            playing.playback.advance(dt);
            let state = world.get::<MoveState>(player_entity);
            if let (Some(transform), Some(player)) = (
                world.get::<PlayerTransform>(player_entity),
                world.get::<Player>(player_entity),
            ) {
                playing
                    .playback
                    .play(&playing.model, Self::player_clip(player, state));
                self.player_yaw.set(next_yaw(
                    self.player_yaw.get(),
                    dt,
                    state.map(|s| s.wish_dir.xz()),
                    player.velocity.xz(),
                ));
                let xform = Mat4::from_scale_rotation_translation(
                    Vec3::splat(PLAYER_SCALE * transform.scale.x),
                    Quat::from_rotation_y(self.player_yaw.get()),
                    transform.translation + PLAYER_OFFSET * transform.scale.y,
                );
                for group in playing.playback.posed(&playing.model, xform) {
                    frame.push(group);
                    player_modelled = true;
                }
            }
        }
        player_modelled
    }
}

/// Yaw the model should ease toward. Prefers the input over velocity so a
/// turnaround faces the stick immediately instead of moonwalking for a frame,
/// as the original did.
fn next_yaw(yaw: f32, dt: f32, wish: Option<Vec2>, velocity: Vec2) -> f32 {
    let wish = wish.unwrap_or(Vec2::ZERO);
    let face = if wish.length_squared() > 1e-4 {
        wish
    } else if velocity.length_squared() > 0.01 {
        velocity
    } else {
        Vec2::ZERO
    };
    if face.length_squared() <= 1e-6 {
        return yaw;
    }
    // This model's forward is local +Z, not -Z. Measured from the file: the leg
    // IK pole targets sit at world z = +0.96, nearly a metre ahead of the hips
    // (z = -0.01), and knees bend toward their pole. The original assumed -Z
    // and so turned the player around.
    let d = face.normalize();
    let target = d.x.atan2(d.y);
    let turn = (1.0 - (-TURN_RATE * dt).exp()).clamp(0.0, 1.0);
    yaw + shortest_angle(yaw, target) * turn
}

fn shortest_angle(from: f32, to: f32) -> f32 {
    (to - from + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}