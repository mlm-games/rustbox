use bevy_ecs::prelude::*;

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
