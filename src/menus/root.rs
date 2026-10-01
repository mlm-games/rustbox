use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::Modifier;
use repose_ui::anim_ext::AnimatedVisibility;
use repose_ui::{ViewExt, ZStack};

use crate::menus::action::UiAction;
use crate::menus::browser::browse_ui;
use crate::menus::components::popup_anim_config;
use crate::menus::dialogs::{load_level_ui, pause_overlay, sign_dialog_ui, sign_editor_ui};
use crate::menus::home::{loading_ui, splash_ui, title_ui};
use crate::menus::{AppState, MenuState, OverlayMenu, ingame_hud, part_picker};

pub fn compose_root(st: &MenuState, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let root = ZStack(Modifier::new().fill_max_size());
    let content = match st.phase {
        AppState::Splash => splash_ui(),
        AppState::Loading => loading_ui(st),
        AppState::Title => ZStack(Modifier::new().fill_max_size()).child((
            title_ui(st, actions.clone()),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Browse,
                browse_ui(st, actions.clone()),
                popup_anim_config("browse"),
            ),
        )),
        AppState::InGame => ZStack(Modifier::new().fill_max_size())
            .child(ingame_hud(st, actions.clone()))
            .child(AnimatedVisibility(
                st.overlay == OverlayMenu::Pause,
                pause_overlay(st, actions.clone()),
                popup_anim_config("pause"),
            ))
            .child(AnimatedVisibility(
                st.overlay == OverlayMenu::LoadLevel,
                load_level_ui(st, actions.clone()),
                popup_anim_config("load_level"),
            ))
            .child(AnimatedVisibility(
                st.overlay == OverlayMenu::PartPicker,
                part_picker(st, actions.clone()),
                popup_anim_config("part_picker"),
            ))
            .child(AnimatedVisibility(
                st.sign_dialog_open && !st.sign_editor_open,
                sign_dialog_ui(st, actions.clone()),
                popup_anim_config("sign_dialog"),
            ))
            .child(AnimatedVisibility(
                st.sign_editor_open,
                sign_editor_ui(st, actions.clone()),
                popup_anim_config("sign_editor"),
            )),
    };
    root.child(content)
}
