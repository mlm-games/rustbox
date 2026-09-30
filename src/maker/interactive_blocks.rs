use bevy_ecs::prelude::*;

use super::Paused;
use super::entities_runtime::{LevelEnt, OnOffSwitch};
use super::interaction::InteractionMemory;
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::win::MakerUi;

/// Independent clock for TimedPulse blocks.
#[derive(Resource)]
pub struct PulseClock {
    /// Seconds the solid phase lasts.
    pub on_secs: f32,
    /// Seconds the empty phase lasts.
    pub off_secs: f32,
    pub elapsed: f32,
    pub solid: bool,
}

impl Default for PulseClock {
    fn default() -> Self {
        Self {
            on_secs: 1.5,
            off_secs: 1.5,
            elapsed: 0.0,
            solid: true,
        }
    }
}

/// Restart the timed pulse phase whenever a run starts (mode switch).
pub fn reset_pulse_clock(mode: Res<MakerMode>, mut clock: ResMut<PulseClock>) {
    if mode.is_changed() && *mode == MakerMode::Play {
        clock.elapsed = 0.0;
        clock.solid = true;
    }
}

/// Global on/off state for On/Off Conveyor A/B blocks. Every OnOffSwitch
/// toggles this same bit; OnOffConveyorA pushes while `on`, B while `!on`.
#[derive(Resource)]
pub struct OnOffState {
    pub on: bool,
}

impl Default for OnOffState {
    fn default() -> Self {
        Self { on: true }
    }
}

/// Reset the global on/off state whenever a run starts (mode switch).
pub fn reset_onoff_state(mode: Res<MakerMode>, mut state: ResMut<OnOffState>) {
    if mode.is_changed() && *mode == MakerMode::Play {
        state.on = true;
    }
}

/// An OnOffSwitch toggles exactly once per touch: the contact latch from
/// `interaction::detect_contacts` fires on entry and re-arms after the actor
/// leaves, so standing on the switch never toggles every frame.
pub fn touch_onoff_switches(world: &mut World) {
    if world.resource::<Paused>().0 {
        return;
    }
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    let entered = {
        let mut q = world.query_filtered::<&LevelEnt, With<OnOffSwitch>>();
        let memory = world.resource::<InteractionMemory>();
        q.iter(world)
            .filter(|ent| memory.any_entered_target(ent.id))
            .count()
    };
    for _ in 0..entered {
        let status = {
            let mut state = world.resource_mut::<OnOffState>();
            state.on = !state.on;
            if state.on {
                "On/Off: ON"
            } else {
                "On/Off: OFF"
            }
        };
        world.resource_mut::<MakerUi>().set_status(status);
    }
}

/// Drive TimedPulse solidity from a free-running clock in Play; always solid
/// in Edit so the blocks build and feel like normal blocks.
pub fn sync_pulse(world: &mut World, dt: f32) {
    if world.resource::<Paused>().0 {
        return;
    }
    let play = *world.resource::<MakerMode>() == MakerMode::Play;
    let on = {
        let mut clock = world.resource_mut::<PulseClock>();
        if !play {
            true
        } else {
            let period = (clock.on_secs + clock.off_secs).max(0.05);
            clock.elapsed = (clock.elapsed + dt) % period;
            clock.solid = clock.elapsed < clock.on_secs.max(0.01);
            clock.solid
        }
    };
    let mut level = world.resource_mut::<LevelDocument>();
    if level.pulse_on != on {
        level.pulse_on = on;
        level.mark_pulse_dirty();
    }
}
