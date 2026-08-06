use std::sync::{Arc, Mutex};

use repose_core::View;
use repose_core::prelude::{Color as RColor, Modifier};
use repose_ui::anim_ext::AnimatedVisibility;
use repose_ui::overlay::OverlayHandle;
use repose_ui::{Column, ViewExt, ZStack};

use crate::app::{AppState, OverlayMenu, SharedUi};
use crate::menus::action::UiAction;
use crate::menus::browser::{browse_ui, online_ui};
use crate::menus::components::popup_anim_config;
use crate::menus::dialogs::{
    credits_ui, level_clear_ui, level_info_ui, level_select_ui, load_level_ui, pause_overlay,
    settings_ui, share_overlay_ui, sign_dialog_ui, sign_editor_ui,
};
use crate::menus::editor::ingame_hud;
use crate::menus::home::{loading_ui, splash_ui, title_ui};

pub fn compose_root(
    overlay: OverlayHandle,
    st: SharedUi,
    actions: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let root = ZStack(Modifier::new().fill_max_size());
    let settings_view = settings_ui(overlay, &st, actions.clone());

    let content = match st.phase {
        AppState::Splash => splash_ui(),
        AppState::Loading => loading_ui(&st),
        AppState::Title => ZStack(Modifier::new().fill_max_size()).child((
            title_ui(&st, actions.clone()),
            AnimatedVisibility(
                st.overlay == OverlayMenu::LevelSelect,
                level_select_ui(&st, actions.clone()),
                popup_anim_config("level_select"),
            ),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Browse,
                browse_ui(&st, actions.clone()),
                popup_anim_config("browse"),
            ),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Online,
                online_ui(&st, actions.clone()),
                popup_anim_config("online"),
            ),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Settings,
                settings_view.clone(),
                popup_anim_config("title_settings"),
            ),
            AnimatedVisibility(
                st.overlay == OverlayMenu::Credits,
                credits_ui(&st, actions.clone()),
                popup_anim_config("title_credits"),
            ),
        )),
        AppState::InGame => {
            let hud = ingame_hud(&st, actions.clone());
            ZStack(Modifier::new().fill_max_size())
                .children(vec![
                    hud,
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::Pause,
                        pause_overlay(&st, actions.clone()),
                        popup_anim_config("pause"),
                    ),
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::Settings,
                        settings_view.clone(),
                        popup_anim_config("ingame_settings"),
                    ),
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::Credits,
                        credits_ui(&st, actions.clone()),
                        popup_anim_config("ingame_credits"),
                    ),
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::LevelClear,
                        level_clear_ui(&st, actions.clone()),
                        popup_anim_config("level_clear"),
                    ),
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::LoadLevel,
                        load_level_ui(&st, actions.clone()),
                        popup_anim_config("load_level"),
                    ),
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::Share,
                        share_overlay_ui(&st, actions.clone()),
                        popup_anim_config("share"),
                    ),
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::LevelInfo,
                        level_info_ui(&st, actions.clone()),
                        popup_anim_config("level_info"),
                    ),
                    AnimatedVisibility(
                        st.overlay == OverlayMenu::PartPicker,
                        crate::menus::editor::part_picker(&st, actions.clone()),
                        popup_anim_config("part_picker"),
                    ),
                ])
                .child(AnimatedVisibility(
                    st.overlay == OverlayMenu::Online,
                    online_ui(&st, actions.clone()),
                    popup_anim_config("ingame_online"),
                ))
                .child(AnimatedVisibility(
                    st.sign_dialog_open && !st.sign_editor_open,
                    sign_dialog_ui(&st, actions.clone()),
                    popup_anim_config("sign_dialog"),
                ))
                .child(AnimatedVisibility(
                    st.sign_editor_open,
                    sign_editor_ui(&st, actions.clone()),
                    popup_anim_config("sign_editor"),
                ))
        }
    };

    if st.transition_alpha > 0.001 || st.flash_alpha > 0.001 {
        let fade_a = (st.transition_alpha.clamp(0.0, 1.0) * 255.0) as u8;
        let flash_a = (st.flash_alpha.clamp(0.0, 1.0) * 255.0) as u8;
        root.child((
            content,
            Column(
                Modifier::new()
                    .fill_max_size()
                    .background(RColor::from_rgba(0, 0, 0, fade_a)),
            ),
            Column(
                Modifier::new()
                    .fill_max_size()
                    .background(RColor::from_rgba(flash_a, flash_a, flash_a, flash_a)),
            ),
        ))
    } else {
        root.child(content)
    }
}
