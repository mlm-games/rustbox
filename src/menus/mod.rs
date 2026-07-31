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
    MakerToggleBrushTab,
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
    if st.maker_mode_edit {
        edit_hud(st, actions)
    } else {
        play_hud(st, actions)
    }
}

// ---------------------------------------------------------------------------
// EDIT MODE
// ---------------------------------------------------------------------------

fn edit_hud(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    ZStack(Modifier::new().fill_max_size()).child((
        Column(
            Modifier::new()
                .fill_max_size()
                .align_items(AlignItems::STRETCH)
                .justify_content(JustifyContent::FLEX_START),
        )
        .child(edit_top_bar(st, actions.clone())),
        Column(
            Modifier::new()
                .fill_max_size()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::FLEX_END)
                .padding(16.0),
        )
        .child(palette_dock(st, actions.clone())),
        Column(
            Modifier::new()
                .fill_max_size()
                .align_items(AlignItems::FLEX_END)
                .justify_content(JustifyContent::CENTER)
                .padding(16.0),
        )
        .child(inspector_panel(st, actions.clone())),
        toast_anchor(st),
    ))
}

fn edit_top_bar(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_mode = actions.clone();
    let a_undo = actions.clone();
    let a_redo = actions.clone();
    let a_save = actions.clone();
    let a_load = actions.clone();
    let a_new = actions.clone();

    Row(Modifier::new()
        .padding(8.0)
        .gap(8.0)
        .align_items(AlignItems::CENTER)
        .background(RColor::from_rgba(15, 15, 22, 235)))
    .children(vec![
        mk_primary_button(
            format!("▶ {}", t(tr, "toolbar-play", "Play")),
            col(60, 160, 90),
            move || push_ui(&a_mode, UiAction::MakerToggleMode),
        ),
        RText(if st.level_name.is_empty() {
            t(tr, "toolbar-level-untitled", "Untitled Level")
        } else {
            st.level_name.clone()
        })
        .size(15.0)
        .color(col(200, 200, 210)),
        Column(Modifier::new().fill_max_width()),
        mk_icon_button("↶", st.can_undo, move || {
            push_ui(&a_undo, UiAction::MakerUndo)
        }),
        mk_icon_button("↷", st.can_redo, move || {
            push_ui(&a_redo, UiAction::MakerRedo)
        }),
        Column(Modifier::new().width(12.0)),
        mk_icon_button("💾", true, move || push_ui(&a_save, UiAction::MakerSave)),
        mk_icon_button("📂", true, move || {
            push_ui(&a_load, UiAction::MakerOpenLoadPanel)
        }),
        mk_icon_button("✚", true, move || {
            push_ui(&a_new, UiAction::MakerNewLevel)
        }),
    ])
}

fn palette_dock(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_tab = actions.clone();

    let tab_label = match st.brush_tab {
        1 => t(tr, "toolbar-entities", "Entities"),
        2 => t(tr, "toolbar-tracks", "Tracks"),
        _ => t(tr, "toolbar-blocks", "Blocks"),
    };

    let swatches: View = match st.brush_tab {
        0 => block_swatches(st, actions.clone()),
        1 => entity_swatches(st, actions.clone()),
        _ => track_hint(st),
    };

    Column(
        Modifier::new()
            .padding(10.0)
            .background(RColor::from_rgba(15, 15, 22, 235))
            .clip_rounded(14.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        Row(Modifier::new().gap(10.0).align_items(AlignItems::CENTER)).child((
            mk_pill_button(format!("⇄ {tab_label} [Q]"), move || {
                push_ui(&a_tab, UiAction::MakerToggleBrushTab)
            }),
            swatches,
        )),
    )
}

fn block_swatches(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let items: [(u8, RColor); 5] = [
        (0, col(90, 170, 90)),
        (1, col(140, 140, 150)),
        (2, col(210, 70, 70)),
        (3, col(230, 195, 70)),
        (4, col(70, 170, 220)),
    ];
    let mut row = Row(Modifier::new().gap(6.0));
    for (idx, color) in items {
        let a = actions.clone();
        row = row.child(mk_swatch(
            format!("{}", idx + 1),
            color,
            st.selected_block == idx,
            move || push_ui(&a, UiAction::MakerSelectBlock(idx)),
        ));
    }
    row
}

fn entity_swatches(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let items: [(u8, RColor); 5] = [
        (0, col(255, 215, 70)),
        (1, col(90, 190, 255)),
        (2, col(190, 90, 230)),
        (3, col(240, 140, 65)),
        (4, col(215, 55, 90)),
    ];
    let a_rot = actions.clone();
    let mut row = Row(Modifier::new().gap(6.0));
    for (idx, color) in items {
        let a = actions.clone();
        row = row.child(mk_swatch(
            format!("{}", idx + 1),
            color,
            st.selected_entity == idx,
            move || push_ui(&a, UiAction::MakerSelectEntity(idx)),
        ));
    }
    row = row.child(mk_pill_button("⟳ F".into(), move || {
        push_ui(&a_rot, UiAction::MakerRotateBrush)
    }));
    row
}

fn track_hint(st: &SharedUi) -> View {
    let tr = &st.translations;
    RText(if st.active_track_label.is_empty() {
        t(
            tr,
            "maker-track-hint-track",
            "LMB add point · RMB remove · Enter finish",
        )
    } else {
        st.active_track_label.clone()
    })
    .size(13.0)
    .color(col(210, 200, 160))
}

fn inspector_panel(st: &SharedUi, _actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let (title, body): (String, Vec<View>) = match st.brush_tab {
        0 => {
            let (key, fallback) = match st.selected_block {
                1 => ("toolbar-stone", "Stone"),
                2 => ("toolbar-hazard", "Hazard"),
                3 => ("toolbar-goal", "Goal"),
                4 => ("toolbar-spawn", "Spawn"),
                _ => ("toolbar-grass", "Grass"),
            };
            (
                t(tr, "inspector-title", "Inspector"),
                vec![
                    RText(t(tr, key, fallback)).size(16.0).color(RColor::WHITE),
                    RText(format!(
                        "{}: {}",
                        t(tr, "toolbar-block-brush", "Block"),
                        st.selected_block + 1
                    ))
                    .size(13.0)
                    .color(col(180, 180, 190)),
                ],
            )
        }
        1 => {
            let (key, fallback, speed) = match st.selected_entity {
                1 => ("maker-ent-pad", "Launch Pad", 14.0),
                2 => ("maker-ent-seal", "Seal", 3.0),
                3 => ("maker-ent-drift", "Drift Plate", 3.0),
                4 => ("toolbar-prowler", "Prowler", 2.5),
                _ => ("maker-ent-glimmer", "Glimmer", 1.0),
            };
            (
                t(tr, "inspector-title", "Inspector"),
                vec![
                    RText(t(tr, key, fallback)).size(16.0).color(RColor::WHITE),
                    RText(format!("Spd {:.1}", speed))
                        .size(13.0)
                        .color(col(180, 180, 190)),
                ],
            )
        }
        _ => {
            let detail = if st.active_track_label.is_empty() {
                t(tr, "maker-track-hint-idle", "No active track")
            } else {
                st.active_track_label.clone()
            };
            (
                t(tr, "inspector-title", "Inspector"),
                vec![RText(detail).size(14.0).color(col(210, 200, 160))],
            )
        }
    };

    Column(
        Modifier::new()
            .width(190.0)
            .padding(12.0)
            .background(RColor::from_rgba(15, 15, 22, 235))
            .clip_rounded(10.0)
            .align_items(AlignItems::FLEX_START),
    )
    .child(RText(title).size(12.0).color(col(150, 150, 165)))
    .children(body)
}

// ---------------------------------------------------------------------------
// PLAY MODE
// ---------------------------------------------------------------------------

fn play_hud(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_edit = actions.clone();

    ZStack(Modifier::new().fill_max_size()).child((
        Column(
            Modifier::new()
                .fill_max_size()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::FLEX_START)
                .padding(10.0),
        )
        .child(
            Row(Modifier::new()
                .padding(8.0)
                .gap(16.0)
                .background(RColor::from_rgba(15, 15, 22, 200))
                .clip_rounded(20.0)
                .align_items(AlignItems::CENTER))
            .child((
                RText(format!("⏱ {:.1}s", st.play_time_secs))
                    .size(16.0)
                    .color(RColor::WHITE),
                RText(format!("☠ {}", st.deaths))
                    .size(16.0)
                    .color(col(230, 120, 120)),
                RText(format!("✦ {}/{}", st.glimmers_collected, st.glimmers_total))
                    .size(16.0)
                    .color(col(255, 220, 110)),
            )),
        ),
        Column(
            Modifier::new()
                .fill_max_size()
                .align_items(AlignItems::FLEX_END)
                .justify_content(JustifyContent::FLEX_START)
                .padding(10.0),
        )
        .child(mk_pill_button(
            format!("✏ {} [Tab]", t(tr, "toolbar-edit", "Edit")),
            move || push_ui(&a_edit, UiAction::MakerToggleMode),
        )),
        toast_anchor(st),
    ))
}

// ---------------------------------------------------------------------------
// Shared pieces
// ---------------------------------------------------------------------------

fn toast_anchor(st: &SharedUi) -> View {
    if st.maker_status.is_empty() {
        return Column(Modifier::new());
    }
    Column(
        Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::FLEX_START)
            .justify_content(JustifyContent::FLEX_END)
            .padding(16.0),
    )
    .child(
        Column(
            Modifier::new()
                .padding(10.0)
                .background(RColor::from_rgba(15, 15, 22, 220))
                .clip_rounded(8.0),
        )
        .child(
            RText(st.maker_status.clone())
                .size(14.0)
                .color(RColor::WHITE),
        ),
    )
}

fn mk_swatch(hotkey: String, color: RColor, selected: bool, on_click: impl Fn() + 'static) -> View {
    let bg = if selected { RColor::WHITE } else { color };
    FilledTonalButton(
        Modifier::new()
            .width(44.0)
            .height(44.0)
            .background(bg)
            .clip_rounded(8.0),
        on_click,
        ButtonConfig::default(),
        move || {
            RText(hotkey.clone()).size(13.0).color(if selected {
                RColor::from_rgba(0, 0, 0, 255)
            } else {
                RColor::WHITE
            })
        },
    )
}

fn mk_icon_button(icon: &str, enabled: bool, on_click: impl Fn() + 'static) -> View {
    let label = icon.to_string();
    FilledTonalButton(
        Modifier::new()
            .width(40.0)
            .height(36.0)
            .background(if enabled {
                col(60, 60, 80)
            } else {
                col(35, 35, 45)
            }),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(17.0),
    )
}

fn mk_pill_button(label: String, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .height(38.0)
            .padding(10.0)
            .clip_rounded(19.0),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(14.0),
    )
}

fn mk_primary_button(label: String, bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .height(38.0)
            .padding(10.0)
            .background(bg)
            .clip_rounded(8.0),
        on_click,
        ButtonConfig::default(),
        move || RText(label.clone()).size(16.0),
    )
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
