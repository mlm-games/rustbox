use bevy::prelude::*;

use super::entities_runtime::{LevelEnt, OnOffSwitch};
use super::interaction::InteractionMemory;
use super::level::LevelDocument;
use super::mode::MakerMode;

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
pub fn touch_onoff_switches(
    mode: Res<MakerMode>,
    memory: Res<InteractionMemory>,
    mut state: ResMut<OnOffState>,
    mut ui: ResMut<super::ui_bridge::MakerUi>,
    switches: Query<&LevelEnt, With<OnOffSwitch>>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    for ent in &switches {
        if memory.any_entered_target(ent.id) {
            state.on = !state.on;
            ui.set_status(if state.on {
                "On/Off: ON"
            } else {
                "On/Off: OFF"
            });
        }
    }
}

pub fn sync_pulse(mode: Res<MakerMode>, state: Res<OnOffState>, mut level: ResMut<LevelDocument>) {
    let on = *mode != MakerMode::Play || state.on;
    if level.pulse_on != on {
        level.pulse_on = on;
        level.mark_pulse_dirty();
    }
}
