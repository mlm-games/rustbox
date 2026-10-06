//! Screen-space feel: the trauma shake and the white flash the gameplay code
//! raises, plus the decay and sampling that turn them into a camera offset
//! and a screen tint.
//!
//! Boundary: gameplay only ever *raises* these amounts, and nothing inside the
//! fixed step reads them, so simulation outcomes never depend on render frame
//! pacing. The decay and the shake sampling run from the frame builder with
//! the render delta. (Moving the amounts themselves out of the world and onto
//! an app-side struct, fed by sim events like `LevelView::place_events`, is the
//! tidier shape if anything ever needs to read them during simulation.)

use bevy_ecs::prelude::{Resource, World};
use glam::Vec3;

/// Fatigue accumulates from hits and decays linearly, exactly like the Bevy
/// original's `ScreenEffectsConfig::trauma_decay`.
const TRAUMA_DECAY: f32 = 1.5;
const SHAKE_MAGNITUDE: f32 = 0.35;

#[derive(Resource, Default)]
pub struct Trauma(pub f32);

impl Trauma {
    pub fn add(&mut self, amount: f32) {
        self.0 = (self.0 + amount).clamp(0.0, 1.0);
    }
}

/// Full-screen white flash, decaying linearly to zero over `duration`.
#[derive(Resource, Default)]
pub struct FlashWhite {
    pub amount: f32,
    elapsed: f32,
    duration: f32,
}

impl FlashWhite {
    pub fn flash(&mut self, duration: f32) {
        self.amount = 1.0;
        self.elapsed = 0.0;
        self.duration = duration;
    }
}

/// xorshift64* so the shake is reproducible frame to frame and needs no OS
/// randomness (the browser build has no entropy source worth blocking on).
#[derive(Resource)]
pub struct ShakeRng(u64);

impl Default for ShakeRng {
    fn default() -> Self {
        Self(0x9E3779B97F4A7C15)
    }
}

impl ShakeRng {
    fn next_unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        ((x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f32 / (1u64 << 53) as f32) * 2.0 - 1.0
    }
}

pub fn tick(world: &mut World, dt: f32) {
    if dt <= 0.0 {
        return;
    }
    let mut trauma = world.resource_mut::<Trauma>();
    trauma.0 = (trauma.0 - TRAUMA_DECAY * dt).max(0.0);
    let mut flash = world.resource_mut::<FlashWhite>();
    if flash.amount > 0.0 {
        flash.elapsed += dt;
        flash.amount = if flash.duration > 0.0 {
            (1.0 - flash.elapsed / flash.duration).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
}

/// Trauma-driven camera offset, sampled symmetrically per axis with the
/// squared trauma the original used (`shake_pow = t * t`).
pub fn shake_offset(world: &mut World) -> Vec3 {
    let trauma = world.resource::<Trauma>().0;
    let magnitude = trauma * trauma * SHAKE_MAGNITUDE;
    if magnitude <= 0.001 {
        return Vec3::ZERO;
    }
    let mut rng = world.resource_mut::<ShakeRng>();
    Vec3::new(
        rng.next_unit() * magnitude,
        rng.next_unit() * magnitude,
        rng.next_unit() * magnitude,
    )
}