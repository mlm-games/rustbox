use bevy::prelude::*;

use super::entities_runtime::OnOffSwitch;
use super::mode::MakerMode;
use super::player::Player;

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

/// The player touching an OnOffSwitch flips the global on/off state.
pub fn touch_onoff_switches(
    mode: Res<MakerMode>,
    mut state: ResMut<OnOffState>,
    mut ui: ResMut<super::ui_bridge::MakerUi>,
    player_q: Query<&Transform, With<Player>>,
    switches: Query<&Transform, (With<OnOffSwitch>, Without<Player>)>,
) {
    if *mode != MakerMode::Play {
        return;
    }
    let Ok(pt) = player_q.single() else {
        return;
    };
    let mut flipped = false;
    for st in &switches {
        if pt.translation.distance(st.translation) < 0.8 {
            flipped = true;
            break;
        }
    }
    if flipped {
        state.on = !state.on;
        ui.set_status(if state.on {
            "On/Off: ON"
        } else {
            "On/Off: OFF"
        });
    }
}
