use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::rc::Rc;

use repose_core::View;
use repose_core::prelude::{
    AlignItems, AnimationSpec, Color as RColor, Easing, JustifyContent, Modifier, remember,
};
use repose_material::material3::{
    ButtonConfig, DropdownMenu, DropdownMenuConfig, DropdownMenuEntry, DropdownMenuItem,
    FilledTonalButton, MenuState,
};
use repose_ui::anim_ext::{
    AnimatedVisibility, AnimatedVisibilityConfig, EnterTransition, ExitTransition,
};
use repose_ui::overlay::OverlayHandle;
use repose_ui::{Column, Row, Text as RText, TextStyle, ViewExt, ZStack};

use crate::app::{AppState, OverlayMenu, SharedUi};

fn t(translations: &HashMap<String, String>, key: &str, fallback: &str) -> String {
    translations
        .get(key)
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

#[derive(Clone, Debug)]
pub enum UiAction {
    StartGame,
    OpenSettings,
    OpenCredits,
    CloseOverlay,
    Resume,
    QuitToTitle,
    QuitApp,
    SetMasterVol(f32),
    SetSfxVol(f32),
    SetMusicVol(f32),
    SaveSettings,
    NextLanguage,
    SetLanguage(String),
    // Maker toolbar
    MakerToggleMode,
    MakerSelectBlock(u8),
    MakerToggleBrush,
    MakerSelectEntity(u8),
    MakerRotateBrush,
    MakerUndo,
    MakerRedo,
    MakerSave,
    MakerLoad,
    MakerNewLevel,
    MakerOpenLoadPanel,
    MakerLoadSlot(String),
    MakerSaveAs(String),
    MakerDismissClear,
    MakerRetry,
    SetPointerOverUi(bool),
}

#[derive(bevy::prelude::Resource, Clone)]
pub struct UiBridge {
    pub shared: Arc<Mutex<SharedUi>>,
    pub actions: Arc<Mutex<Vec<UiAction>>>,
}

fn spacer(h: f32) -> View {
    Column(Modifier::new().height(h).width(1.0))
}

fn popup_anim_config(key: &str) -> AnimatedVisibilityConfig {
    AnimatedVisibilityConfig {
        key: key.into(),
        spec: AnimationSpec::tween(Duration::from_millis(200), Easing::EaseOut),
        enter: EnterTransition::ScaleIn { initial: 0.95 },
        exit: ExitTransition::ScaleOut { target: 0.95 },
    }
}

pub fn compose_root(
    overlay: OverlayHandle,
    st: SharedUi,
    actions: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let root = ZStack(Modifier::new().fill_max_size());
    let settings_view = settings_ui(overlay, &st, actions.clone());

    let content = match st.phase {
        AppState::Splash => splash_ui(),
        AppState::Loading => loading_ui(),
        AppState::Title => ZStack(Modifier::new().fill_max_size()).child((
            title_ui(&st, actions.clone()),
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
            ZStack(Modifier::new().fill_max_size()).child((
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

fn splash_ui() -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(col(8, 8, 12)),
    )
    .child(RText("My Ecosystem").size(48.0).color(RColor::WHITE))
}

fn loading_ui() -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(col(8, 8, 12)),
    )
    .child(RText("Loading...").size(32.0).color(RColor::WHITE))
}

fn title_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a2 = actions.clone();
    let a3 = actions.clone();
    let a4 = actions.clone();
    let tr = &st.translations;

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(col(8, 8, 12)),
    )
    .child((
        RText(t(tr, "app-title", "My Ecosystem Bevy"))
            .size(56.0)
            .color(RColor::WHITE),
        spacer(24.0),
        mk_button(
            &t(tr, "start-game", "Start Game"),
            col(60, 120, 200),
            move || push(&a1, UiAction::StartGame),
        ),
        mk_button(&t(tr, "settings", "Settings"), col(70, 70, 90), move || {
            push(&a2, UiAction::OpenSettings)
        }),
        mk_button(&t(tr, "credits", "Credits"), col(70, 70, 90), move || {
            push(&a3, UiAction::OpenCredits)
        }),
        mk_button(&t(tr, "quit", "Quit"), col(180, 60, 60), move || {
            push(&a4, UiAction::QuitApp)
        }),
    ))
}

fn pause_overlay(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a2 = actions.clone();
    let a3 = actions.clone();
    let tr = &st.translations;

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(pause_panel(tr, a1, a2, a3))
}

fn pause_panel(
    tr: &HashMap<String, String>,
    a1: Arc<Mutex<Vec<UiAction>>>,
    a2: Arc<Mutex<Vec<UiAction>>>,
    a3: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    Column(
        Modifier::new()
            .width(320.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "paused", "Paused"))
            .size(36.0)
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "resume", "Resume"), col(60, 140, 90), move || {
            push(&a1, UiAction::Resume)
        }),
        mk_button(&t(tr, "settings", "Settings"), col(70, 70, 90), move || {
            push(&a2, UiAction::OpenSettings)
        }),
        mk_button(
            &t(tr, "quit-to-title", "Quit to Title"),
            col(180, 60, 60),
            move || push(&a3, UiAction::QuitToTitle),
        ),
    ))
}

fn settings_ui(overlay: OverlayHandle, st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_m_down = actions.clone();
    let a_m_up = actions.clone();
    let a_s_down = actions.clone();
    let a_s_up = actions.clone();
    let a_mu_down = actions.clone();
    let a_mu_up = actions.clone();
    let a_save = actions.clone();
    let a_back = actions.clone();
    let master = st.master_vol;
    let sfx = st.sfx_vol;
    let music = st.music_vol;
    let tr = &st.translations;
    let lang = &st.language;
    let langs = &st.available_languages;
    let overlay_clone = overlay.clone();
    let actions_clone = actions.clone();

    let menu_state: Rc<MenuState> = remember(MenuState::new);
    let lang_items: Vec<DropdownMenuEntry> = langs
        .iter()
        .map(|l| {
            let a = actions_clone.clone();
            let code = l.clone();
            let mut item = DropdownMenuItem::new(l.clone(), move || {
                push(&a, UiAction::SetLanguage(code.clone()))
            });
            if l == lang {
                item = item.disabled();
            }
            DropdownMenuEntry::Item(item)
        })
        .collect();
    let menu_trigger = menu_state.clone();
    let lang_label = st.language.clone();
    let trigger = FilledTonalButton(
        Modifier::new().width(100.0).height(40.0),
        move || menu_trigger.open(),
        ButtonConfig::default(),
        move || RText(lang_label.clone()).size(20.0),
    );

    let lang_dropdown = DropdownMenu(
        menu_state,
        overlay_clone,
        Modifier::new(),
        trigger,
        lang_items,
        DropdownMenuConfig {
            min_width: 100.0,
            ..Default::default()
        },
    );

    let inner = Column(
        Modifier::new()
            .width(360.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "settings", "Settings"))
            .size(36.0)
            .color(RColor::WHITE),
    )
    .child(spacer(12.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "master-volume", "Master"),
            master * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || {
            push(&a_m_down, UiAction::SetMasterVol(master - 0.1))
        }),
        mk_button_sm("+", move || {
            push(&a_m_up, UiAction::SetMasterVol(master + 0.1))
        }),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "sfx-volume", "SFX"),
            sfx * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || push(&a_s_down, UiAction::SetSfxVol(sfx - 0.1))),
        mk_button_sm("+", move || push(&a_s_up, UiAction::SetSfxVol(sfx + 0.1))),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!(
            "{}: {:.0}%",
            t(tr, "music-volume", "Music"),
            music * 100.0
        ))
        .size(18.0)
        .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(8.0)).child((
        mk_button_sm("-", move || {
            push(&a_mu_down, UiAction::SetMusicVol(music - 0.1))
        }),
        mk_button_sm("+", move || {
            push(&a_mu_up, UiAction::SetMusicVol(music + 0.1))
        }),
    )))
    .child(spacer(8.0))
    .child(
        RText(format!("{}:", t(tr, "language", "Language")))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(Row(Modifier::new().gap(6.0)).child(lang_dropdown))
    .child(spacer(16.0))
    .child(mk_button(
        &t(tr, "save", "Save"),
        col(60, 120, 200),
        move || push(&a_save, UiAction::SaveSettings),
    ))
    .child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_back, UiAction::CloseOverlay),
    ));

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(inner)
}

fn credits_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a = actions.clone();
    let tr = &st.translations;
    let inner = Column(
        Modifier::new()
            .width(400.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "credits", "Credits"))
            .size(36.0)
            .color(RColor::WHITE),
        spacer(12.0),
        RText("Original Godot template: mlm-games")
            .size(16.0)
            .color(RColor::WHITE),
        RText("Bevy + Repose port: mlm-games")
            .size(16.0)
            .color(RColor::WHITE),
        RText("Engine: Bevy  UI: Repose")
            .size(16.0)
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "back", "Back"), col(70, 70, 90), move || {
            push(&a, UiAction::CloseOverlay)
        }),
    ));

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(inner)
}

fn ingame_hud(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;

    Column(
        Modifier::new()
            .fill_max_size()
            .padding(16.0)
            .align_items(AlignItems::FLEX_START)
            .justify_content(JustifyContent::FLEX_START),
    )
    .child((
        maker_toolbar_top(st, tr, actions.clone()),
        spacer(8.0),
        maker_toolbar_palette(st, tr, actions.clone()),
        spacer(10.0),
        maker_stats_panel(st, tr),
    ))
}

fn maker_toolbar_top(
    st: &SharedUi,
    tr: &HashMap<String, String>,
    actions: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let a_mode = actions.clone();
    let a_brush = actions.clone();
    let a_undo = actions.clone();
    let a_redo = actions.clone();
    let a_save = actions.clone();
    let a_load = actions.clone();
    let a_new = actions.clone();

    Column(
        Modifier::new()
            .padding(10.0)
            .background(RColor::from_rgba(20, 20, 28, 220))
            .clip_rounded(10.0)
            .align_items(AlignItems::FLEX_START),
    )
    .child((
        Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child((
            mk_tool_button(
                if st.maker_mode_edit {
                    t(tr, "toolbar-play", "Play")
                } else {
                    t(tr, "toolbar-edit", "Edit")
                },
                if st.maker_mode_edit {
                    col(60, 140, 90)
                } else {
                    col(60, 120, 200)
                },
                move || push_ui(&a_mode, UiAction::MakerToggleMode),
            ),
            mk_tool_button(
                if st.brush_entities {
                    t(tr, "toolbar-entities", "Entities")
                } else {
                    t(tr, "toolbar-blocks", "Blocks")
                },
                col(90, 90, 120),
                move || push_ui(&a_brush, UiAction::MakerToggleBrush),
            ),
            mk_tool_button(
                t(tr, "maker-undo", "Undo"),
                if st.can_undo {
                    col(90, 90, 120)
                } else {
                    col(50, 50, 60)
                },
                move || push_ui(&a_undo, UiAction::MakerUndo),
            ),
            mk_tool_button(
                t(tr, "maker-redo", "Redo"),
                if st.can_redo {
                    col(90, 90, 120)
                } else {
                    col(50, 50, 60)
                },
                move || push_ui(&a_redo, UiAction::MakerRedo),
            ),
            mk_tool_button(t(tr, "maker-save", "Save"), col(70, 110, 170), move || {
                push_ui(&a_save, UiAction::MakerSave)
            }),
            mk_tool_button(
                t(tr, "maker-load", "Load..."),
                col(70, 110, 170),
                move || push_ui(&a_load, UiAction::MakerOpenLoadPanel),
            ),
            mk_tool_button(t(tr, "maker-new", "New"), col(150, 80, 70), move || {
                push_ui(&a_new, UiAction::MakerNewLevel)
            }),
        )),
        spacer(6.0),
        RText(if st.level_name.is_empty() {
            t(tr, "toolbar-level-untitled", "Untitled Level")
        } else {
            st.level_name.clone()
        })
        .size(14.0)
        .color(col(220, 220, 230)),
    ))
}

fn maker_toolbar_palette(
    st: &SharedUi,
    tr: &HashMap<String, String>,
    actions: Arc<Mutex<Vec<UiAction>>>,
) -> View {
    if !st.brush_entities {
        let a0 = actions.clone();
        let a1 = actions.clone();
        let a2 = actions.clone();
        let a3 = actions.clone();
        let a4 = actions.clone();
        Column(
            Modifier::new()
                .padding(10.0)
                .background(RColor::from_rgba(20, 20, 28, 220))
                .clip_rounded(10.0)
                .align_items(AlignItems::FLEX_START),
        )
        .child((
            RText(t(tr, "toolbar-block-brush", "Block Brush"))
                .size(14.0)
                .color(col(220, 220, 230)),
            spacer(6.0),
            Row(Modifier::new().gap(8.0)).child((
                mk_select_button(
                    t(tr, "toolbar-grass", "Grass"),
                    col(70, 120, 70),
                    st.selected_block == 0,
                    move || push_ui(&a0, UiAction::MakerSelectBlock(0)),
                ),
                mk_select_button(
                    t(tr, "toolbar-stone", "Stone"),
                    col(120, 120, 130),
                    st.selected_block == 1,
                    move || push_ui(&a1, UiAction::MakerSelectBlock(1)),
                ),
                mk_select_button(
                    t(tr, "toolbar-hazard", "Hazard"),
                    col(180, 60, 60),
                    st.selected_block == 2,
                    move || push_ui(&a2, UiAction::MakerSelectBlock(2)),
                ),
                mk_select_button(
                    t(tr, "toolbar-goal", "Goal"),
                    col(200, 170, 60),
                    st.selected_block == 3,
                    move || push_ui(&a3, UiAction::MakerSelectBlock(3)),
                ),
                mk_select_button(
                    t(tr, "toolbar-spawn", "Spawn"),
                    col(60, 160, 200),
                    st.selected_block == 4,
                    move || push_ui(&a4, UiAction::MakerSelectBlock(4)),
                ),
            )),
        ))
    } else {
        let a0 = actions.clone();
        let a1 = actions.clone();
        let a2 = actions.clone();
        let a3 = actions.clone();
        let a_rot = actions.clone();
        Column(
            Modifier::new()
                .padding(10.0)
                .background(RColor::from_rgba(20, 20, 28, 220))
                .clip_rounded(10.0)
                .align_items(AlignItems::FLEX_START),
        )
        .child((
            RText(t(tr, "toolbar-entity-brush", "Entity Brush"))
                .size(14.0)
                .color(col(220, 220, 230)),
            spacer(6.0),
            Row(Modifier::new().gap(8.0)).child((
                mk_select_button(
                    t(tr, "maker-ent-glimmer", "Glimmer"),
                    col(220, 190, 60),
                    st.selected_entity == 0,
                    move || push_ui(&a0, UiAction::MakerSelectEntity(0)),
                ),
                mk_select_button(
                    t(tr, "maker-ent-pad", "Launch Pad"),
                    col(70, 160, 220),
                    st.selected_entity == 1,
                    move || push_ui(&a1, UiAction::MakerSelectEntity(1)),
                ),
                mk_select_button(
                    t(tr, "maker-ent-seal", "Seal"),
                    col(170, 80, 210),
                    st.selected_entity == 2,
                    move || push_ui(&a2, UiAction::MakerSelectEntity(2)),
                ),
                mk_select_button(
                    t(tr, "maker-ent-drift", "Drift Plate"),
                    col(220, 120, 60),
                    st.selected_entity == 3,
                    move || push_ui(&a3, UiAction::MakerSelectEntity(3)),
                ),
                mk_tool_button(
                    t(tr, "toolbar-rotate", "Rotate"),
                    col(90, 90, 120),
                    move || push_ui(&a_rot, UiAction::MakerRotateBrush),
                ),
            )),
        ))
    }
}

fn maker_stats_panel(st: &SharedUi, tr: &HashMap<String, String>) -> View {
    Column(
        Modifier::new()
            .padding(10.0)
            .background(RColor::from_rgba(20, 20, 28, 220))
            .clip_rounded(10.0)
            .align_items(AlignItems::FLEX_START),
    )
    .child((
        RText(if st.maker_mode_edit {
            t(tr, "maker-mode-edit", "Mode: EDIT")
        } else {
            t(tr, "maker-mode-play", "Mode: PLAY")
        })
        .size(14.0)
        .color(RColor::WHITE),
        spacer(4.0),
        RText(format!(
            "{}: {}",
            t(tr, "maker-blocks-count", "Blocks"),
            st.blocks_placed
        ))
        .size(14.0)
        .color(col(220, 220, 230)),
        RText(format!(
            "{}: {}/{}",
            t(tr, "maker-glimmers-count", "Glimmers"),
            st.glimmers_collected,
            st.glimmers_total
        ))
        .size(14.0)
        .color(col(255, 220, 100)),
        spacer(4.0),
        RText(if !st.maker_status.is_empty() {
            st.maker_status.clone()
        } else if st.maker_mode_edit {
            if st.brush_entities {
                t(
                    tr,
                    "maker-hint-entity",
                    "Q brush · F yaw · LMB place · RMB erase",
                )
            } else {
                t(
                    tr,
                    "maker-hint-block",
                    "Tab play · 1-5 block · LMB place · RMB erase",
                )
            }
        } else {
            t(
                tr,
                "maker-hint-play",
                "WASD move · Space jump · collect Glimmers · Goal",
            )
        })
        .size(13.0)
        .color(col(170, 170, 180)),
    ))
}

fn level_clear_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_edit = actions.clone();
    let a_retry = actions.clone();

    let inner = Column(
        Modifier::new()
            .width(380.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "maker-clear-title", "Level Clear!"))
            .size(38.0)
            .color(RColor::WHITE),
        spacer(12.0),
        RText(format!(
            "{}: {:.2}s",
            t(tr, "maker-time", "Time"),
            st.clear_time_secs
        ))
        .size(18.0)
        .color(RColor::WHITE),
        RText(format!(
            "{}: {}",
            t(tr, "maker-deaths", "Deaths"),
            st.clear_deaths
        ))
        .size(18.0)
        .color(RColor::WHITE),
        RText(format!(
            "{}: {}/{}",
            t(tr, "maker-glimmers-count", "Glimmers"),
            st.glimmers_collected,
            st.glimmers_total
        ))
        .size(18.0)
        .color(col(255, 220, 100)),
        RText(format!(
            "{}: {}",
            t(tr, "maker-blocks-count", "Blocks"),
            st.blocks_placed
        ))
        .size(18.0)
        .color(RColor::WHITE),
    ))
    .child(spacer(16.0))
    .child(mk_button(
        &t(tr, "maker-btn-edit", "Edit Level"),
        col(70, 110, 170),
        move || push_ui(&a_edit, UiAction::MakerDismissClear),
    ))
    .child(mk_button(
        &t(tr, "maker-retry", "Retry"),
        col(60, 140, 90),
        move || push_ui(&a_retry, UiAction::MakerRetry),
    ));

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(inner)
}

fn load_level_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_back = actions.clone();

    let mut slot_views: Vec<View> = Vec::new();
    if st.level_slots.is_empty() {
        slot_views.push(
            RText(t(tr, "maker-load-empty", "No saved levels"))
                .size(16.0)
                .color(col(180, 180, 190)),
        );
    } else {
        for name in st.level_slots.iter().take(12) {
            let a = actions.clone();
            let n = name.clone();
            let label = name.clone();
            slot_views.push(mk_button(&label, col(70, 70, 90), move || {
                push_ui(&a, UiAction::MakerLoadSlot(n.clone()))
            }));
        }
    }

    let inner = Column(
        Modifier::new()
            .width(380.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "maker-load-title", "Load Level"))
            .size(32.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.level_name.is_empty() {
            String::new()
        } else {
            format!("{}: {}", t(tr, "maker-current", "Current"), st.level_name)
        })
        .size(14.0)
        .color(col(180, 180, 190)),
    )
    .child(spacer(12.0))
    .child(slot_views)
    .child(spacer(12.0))
    .child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push_ui(&a_back, UiAction::CloseOverlay),
    ));

    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180)),
    )
    .child(inner)
}

fn mk_tool_button(label: String, bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .width(76.0)
            .height(36.0)
            .padding(6.0)
            .background(bg),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(15.0),
    )
}

fn mk_select_button(
    label: String,
    base: RColor,
    selected: bool,
    on_click: impl Fn() + 'static,
) -> View {
    let bg = if selected {
        base.with_alpha_f32(0.35).composite_over(RColor::WHITE)
    } else {
        base
    };
    FilledTonalButton(
        Modifier::new()
            .width(86.0)
            .height(36.0)
            .padding(6.0)
            .background(bg),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(15.0),
    )
}

fn mk_button(label: &str, _bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new().width(260.0).height(52.0).margin(8.0),
        on_click,
        ButtonConfig::default(),
        move || RText(label).size(20.0),
    )
}

fn mk_button_sm(label: &str, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new().width(48.0).height(40.0),
        on_click,
        ButtonConfig::default(),
        move || RText(label).size(20.0),
    )
}

fn col(r: u8, g: u8, b: u8) -> RColor {
    RColor::from_rgba(r, g, b, 255)
}

fn push(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    if let Ok(mut q) = actions.lock() {
        q.push(a);
    }
}

fn push_ui(actions: &Arc<Mutex<Vec<UiAction>>>, a: UiAction) {
    push(actions, UiAction::SetPointerOverUi(true));
    push(actions, a);
}
