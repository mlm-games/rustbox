use bevy::prelude::*;
use game_utils_bevy::transitions::Transition;

pub struct DevToolsPlugin;
impl Plugin for DevToolsPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "dev")]
        {
            app.add_systems(Update, log_state_change);
            app.add_systems(Update, debug_jump_to_level);
        }
    }
}

#[cfg(feature = "dev")]
fn debug_jump_to_level(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<crate::app::AppState>>,
    mut tr: ResMut<Transition<crate::app::AppState>>,
    mut maker_ui: Option<ResMut<crate::maker::ui_bridge::MakerUi>>,
) {
    if *state.get() != crate::app::AppState::Title {
        return;
    }
    if keys.just_pressed(KeyCode::F8) {
        if let Some(ref mut m) = maker_ui {
            m.commands
                .push(crate::maker::ui_bridge::UiCommand::PlayBundled(0));
        }
        tr.begin_to_state(crate::app::AppState::Loading);
    }
}

#[cfg(feature = "dev")]
fn log_state_change(state: Res<State<crate::app::AppState>>) {
    if state.is_changed() {
        bevy::log::info!("AppState  {:?}", state.get());
    }
}
