use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use repose_core::View;
use repose_core::prelude::{
    AlignItems, AlignSelf, AnimationSpec, Color as RColor, Easing, JustifyContent, Modifier,
    remember,
};
use repose_core::{
    ImeAction, KeyboardOptions, KeyboardType, StateColors, StateElevation,
    TextFieldLineLimits,
};
use repose_material::material3::{
    ButtonConfig, CardConfig, ChipConfig, ClickableOutlinedCard, DropdownMenu,
    DropdownMenuConfig, DropdownMenuEntry, DropdownMenuItem, FilledTonalButton,
    FilledTonalIconButton, IconButtonColors, IconButtonConfig, InputChip, MenuState,
    OutlinedTextField, OutlinedTextFieldConfig,
};
use repose_material::{Icon, Symbol, material_symbols};
use repose_ui::anim_ext::{
    AnimatedVisibility, AnimatedVisibilityConfig, EnterTransition, ExitTransition,
};
use repose_ui::overlay::OverlayHandle;
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::{
    BasicTextField, Column, Row, Text as RText, TextFieldConfig, TextFieldState, TextStyle,
    ViewExt, ZStack,
};

use crate::app::{AppState, OverlayMenu, SharedUi};
use crate::maker::catalog::{LevelSourceKind, LevelSummary, difficulty_label};
use crate::maker::entity_data::EntityKind;
use crate::maker::level::LevelTag;
use crate::maker::thumbnail::ThumbPreview;
use crate::maker::track::TrackMode;
use rustbox_format::api::LevelMeta;

material_symbols! {
    ADD: '\u{E145}',
    AUTO_AWESOME: '\u{E65F}',
    CHECK: '\u{E5CA}',
    CLOUD: '\u{E2BD}',
    CLOUD_UPLOAD: '\u{E2C3}',
    DELETE: '\u{E872}',
    EDIT: '\u{F097}',
    FLAG: '\u{E153}',
    FOLDER_OPEN: '\u{E2C8}',
    INFO: '\u{E88E}',
    LINK: '\u{E250}',
    PLAY_ARROW: '\u{E037}',
    PUBLISH: '\u{E255}',
    REDO: '\u{E15A}',
    REFRESH: '\u{E5D5}',
    REMOVE: '\u{E15B}',
    ROTATE_RIGHT: '\u{E41A}',
    SEARCH: '\u{E8B6}',
    SAVE: '\u{E161}',
    SKULL: '\u{F89A}',
    STAR: '\u{F09A}',
    SWAP_HORIZ: '\u{E8D4}',
    THUMB_UP: '\u{E8DC}',
    TIMER: '\u{E425}',
    UNDO: '\u{E166}',
}

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
    OpenLevelSelect,
    PlayBundledLevel(u8),
    MakerRemix,
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
    MakerRotateBrushBlock,
    MakerCycleShape,
    MakerToggleWaterlog,
    MakerCycleLinkChannel,
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
    MakerPublish,
    MakerExportCode,
    MakerImportCode(String),
    MakerCopyCode,
    MakerInspParamDelta(f32),
    MakerInspYawDelta(f32),
    MakerInspLinkDelta(i32),
    MakerInspTrackCycle,
    MakerInspDeleteEntity,
    MakerInspTrackModeToggle,
    MakerInspTrackSpeedDelta(f32),
    MakerInspTrackReverse,
    MakerInspTrackDelete,
    BrowseOpen,
    BrowsePlay(String),
    BrowseEdit(String),
    BrowseDelete(String),
    BrowseConfirmDelete(String),
    BrowseCancelDelete,
    BrowseSelect(String),
    BrowseClearSelection,
    BrowseToggleTag(LevelTag),
    BrowseToggleVerified,
    BrowseSetDifficulty(Option<u8>),
    BrowseCycleSort,
    BrowseSetQuery(String),
    BrowseClearQuery,
    BrowseAddToCollection,
    OnlineOpen,
    OnlineRefresh,
    OnlinePlay(u64),
    OnlineLike(u64),
    OnlineReport(u64),
    OnlineDelete(u64),
    OnlineUpload,
    OnlineSelect(u64),
    OnlineClearSelection,
    /// Request a background download of level data purely to generate a card preview.
    OnlinePreview(u64),
    OnlineSetShelf(u8),
    OnlineSetIdQuery(String),
    OnlineSearchId,
    OnlineSetToken(String),
    OnlineSetQuery(String),
    OnlineSearch,
    OnlineClearQuery,
    OnlineCycleSort,
    LevelInfoOpen,
    LevelInfoFocus(u8),
    LevelInfoToggleTag(LevelTag),
    LevelInfoSave,
    LevelInfoClose,
    /// Apply one of the named boundary presets (floor / walls / ceiling).
    LevelInfoPreset(crate::maker::level::BoundaryPreset),
    /// Adjust the global water plane by `delta` cells (applies immediately).
    LevelInfoWaterDelta(i32),
    /// Grow/shrink the play-size half-extents by `delta` on every axis.
    LevelInfoSizeDelta(i32),
    /// Revert the play-size back to auto (derived from content).
    LevelInfoSizeAuto,
    /// Raise/lower the boundary wall height by `delta` cells.
    LevelInfoHeightDelta(i32),
    /// Revert the boundary wall height to auto (from level size).
    LevelInfoHeightAuto,
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

/// HACK: Full-screen dimmer that blocks pointer hits to everything underneath.
/// Background alone is paint-only; the clickable + focusable(false) create a
/// real hit region so buttons under the modal don't stay clickable/hoverable.
/// Using Overlay host dialogs, from repose-material is better than this
fn modal_shell(inner: View) -> View {
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(RColor::from_rgba(0, 0, 0, 180))
            .clickable()
            .focusable(false),
    )
    .child(inner)
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
                .child((
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
                ))
                .child(AnimatedVisibility(
                    st.overlay == OverlayMenu::Online,
                    online_ui(&st, actions.clone()),
                    popup_anim_config("ingame_online"),
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

fn loading_ui(st: &SharedUi) -> View {
    let pct = st.loading_progress.clamp(0.0, 1.0);
    Column(
        Modifier::new()
            .fill_max_size()
            .justify_content(JustifyContent::CENTER)
            .align_items(AlignItems::CENTER)
            .background(col(8, 8, 12)),
    )
    .child(RText("Loading...").size(32.0).color(RColor::WHITE))
    .child(spacer(16.0))
    .child(
        RText(format!("{:.0}%", pct * 100.0))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(spacer(12.0))
    .child(
        Column(
            Modifier::new()
                .width(320.0)
                .height(12.0)
                .background(col(30, 30, 38))
                .clip_rounded(6.0),
        )
        .child(Column(
            Modifier::new()
                .width((320.0 * pct).max(1.0))
                .height(12.0)
                .background(col(96, 165, 250))
                .clip_rounded(6.0)
                .align_self(AlignSelf::FLEX_START),
        )),
    )
}

fn title_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_create = actions.clone();
    let a_levels = actions.clone();
    let a_browse = actions.clone();
    let a_online = actions.clone();
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
    .child(
        RText(t(tr, "app-title", "My Ecosystem Bevy"))
            .size(56.0)
            .color(RColor::WHITE),
    )
    .child(spacer(24.0))
    .child(mk_button(
        &t(tr, "create", "Create"),
        col(60, 120, 200),
        move || push(&a_create, UiAction::StartGame),
    ))
    .child(mk_button(
        &t(tr, "play-levels", "Play Levels"),
        col(80, 170, 120),
        move || push(&a_levels, UiAction::OpenLevelSelect),
    ))
    .child(mk_button("Browse Levels", col(150, 110, 200), move || {
        push(&a_browse, UiAction::BrowseOpen)
    }))
    .child(mk_button("Online Levels", col(90, 160, 210), move || {
        push(&a_online, UiAction::OnlineOpen)
    }))
    .child(mk_button(
        &t(tr, "settings", "Settings"),
        col(70, 70, 90),
        move || push(&a2, UiAction::OpenSettings),
    ))
    .child(mk_button(
        &t(tr, "credits", "Credits"),
        col(70, 70, 90),
        move || push(&a3, UiAction::OpenCredits),
    ))
    .child(mk_button(
        &t(tr, "quit", "Quit"),
        col(180, 60, 60),
        move || push(&a4, UiAction::QuitApp),
    ))
}

fn level_select_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_back = actions.clone();

    let row = |i: u8, lvl: &crate::maker::campaign::CampaignLevelUi| -> View {
        let a = actions.clone();
        let title = lvl.title.clone();
        let teaches = lvl.teaches.clone();
        let status = if lvl.completed {
            match (lvl.best_time, lvl.best_deaths) {
                (Some(best), Some(d)) => icon_text(
                    Symbols::CHECK,
                    format!("{:.1}s · {} {}", best, d, t(tr, "maker-deaths", "deaths")),
                    12.0,
                    col(220, 210, 120),
                ),
                _ => RText(t(tr, "completed", "Completed"))
                    .size(12.0)
                    .color(col(220, 210, 120)),
            }
        } else {
            RText(t(tr, "uncleared", "Uncleared"))
                .size(12.0)
                .color(col(120, 125, 140))
        };
        Column(Modifier::new().align_items(AlignItems::CENTER)).child((
            mk_button(
                &format!("{}. {}", i + 1, title),
                col(70, 90, 120),
                move || push(&a, UiAction::PlayBundledLevel(i)),
            ),
            RText(teaches).size(12.0).color(col(160, 165, 180)),
            status,
            spacer(4.0),
        ))
    };

    let inner = Column(
        Modifier::new()
            .width(420.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        RText(t(tr, "level-select-title", "Play Levels"))
            .size(32.0)
            .color(RColor::WHITE),
        spacer(12.0),
    ));

    let mut inner = inner;
    for (i, lvl) in st.campaign_levels.iter().enumerate() {
        inner = inner.child(row(i as u8, lvl));
    }

    inner = inner.child(spacer(12.0)).child(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push(&a_back, UiAction::CloseOverlay),
    ));

    modal_shell(inner)
}

fn pause_overlay(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a1 = actions.clone();
    let a2 = actions.clone();
    let a3 = actions.clone();
    let tr = &st.translations;

    modal_shell(pause_panel(tr, a1, a2, a3))
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

    modal_shell(inner)
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
        RText("3D models: Quaternius Cubeworld Kit (CC0)")
            .size(16.0)
            .color(RColor::WHITE),
        spacer(16.0),
        mk_button(&t(tr, "back", "Back"), col(70, 70, 90), move || {
            push(&a, UiAction::CloseOverlay)
        }),
    ));

    modal_shell(inner)
}

fn ingame_hud(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    if st.maker_mode_edit {
        edit_hud(st, actions)
    } else {
        play_hud(st, actions)
    }
}

// EDIT MODE

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
    let a_info = actions.clone();
    let a_new = actions.clone();
    let a_pub = actions.clone();
    let a_online = actions.clone();

    Row(Modifier::new()
        .padding(8.0)
        .gap(8.0)
        .align_items(AlignItems::CENTER)
        .background(RColor::from_rgba(15, 15, 22, 235)))
    .children(vec![
        mk_primary_button(
            icon_label(Symbols::PLAY_ARROW, t(tr, "toolbar-play", "Play")),
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
        RText(if st.level_verified {
            t(tr, "toolbar-verified", "Verified")
        } else {
            t(tr, "toolbar-unverified", "Unverified")
        })
        .size(13.0)
        .color(if st.level_verified {
            col(90, 200, 120)
        } else {
            col(230, 160, 70)
        }),
        Column(Modifier::new().fill_max_width()),
        mk_icon_button(Symbols::UNDO, st.can_undo, move || {
            push_ui(&a_undo, UiAction::MakerUndo)
        }),
        mk_icon_button(Symbols::REDO, st.can_redo, move || {
            push_ui(&a_redo, UiAction::MakerRedo)
        }),
        Column(Modifier::new().width(12.0)),
        mk_icon_button(Symbols::SAVE, true, move || {
            push_ui(&a_save, UiAction::MakerSave)
        }),
        mk_icon_button(Symbols::FOLDER_OPEN, true, move || {
            push_ui(&a_load, UiAction::MakerOpenLoadPanel)
        }),
        mk_icon_button(Symbols::INFO, true, move || {
            push_ui(&a_info, UiAction::LevelInfoOpen)
        }),
        mk_icon_button(Symbols::ADD, true, move || {
            push_ui(&a_new, UiAction::MakerNewLevel)
        }),
        mk_icon_button(Symbols::PUBLISH, true, move || {
            push_ui(&a_pub, UiAction::MakerPublish)
        }),
        mk_icon_button(Symbols::CLOUD, true, move || {
            push_ui(&a_online, UiAction::OnlineOpen)
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
            mk_pill_button(
                icon_label(Symbols::SWAP_HORIZ, format!("{tab_label} [Q]")),
                move || push_ui(&a_tab, UiAction::MakerToggleBrushTab),
            ),
            swatches,
        )),
    )
}

fn block_swatches(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let items: [(u8, RColor); 6] = [
        (0, col(90, 170, 90)),
        (1, col(140, 140, 150)),
        (2, col(210, 70, 70)),
        (3, col(230, 195, 70)),
        (4, col(70, 170, 220)),
        (5, col(50, 130, 230)),
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
    let a_shape = actions.clone();
    let a_rot = actions.clone();
    let a_log = actions.clone();
    let shapes = [
        "Full", "Half", "Top", "Slope", "DSlope", "Corner", "O.Corner", "V.Slope", "V.Slab",
    ];
    let shape = shapes
        .get(st.brush_shape as usize)
        .copied()
        .unwrap_or("Full");
    let log = if st.waterlogged {
        format!("{} · {}° · wet", shape, st.brush_rot * 90)
    } else {
        format!("{} · {}°", shape, st.brush_rot * 90)
    };
    row = row
        .child(mk_pill_button(
            icon_label(Symbols::REFRESH, format!("{shape} [T]")),
            move || push_ui(&a_shape, UiAction::MakerCycleShape),
        ))
        .child(mk_pill_button(
            icon_label(Symbols::ROTATE_RIGHT, format!("{}° [R]", st.brush_rot * 90)),
            move || push_ui(&a_rot, UiAction::MakerRotateBrushBlock),
        ))
        .child(mk_pill_button(
            icon_label(Symbols::CLOUD, format!("{log} [U]")),
            move || push_ui(&a_log, UiAction::MakerToggleWaterlog),
        ));
    row
}

fn entity_swatches(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let items: [(u8, RColor); 7] = [
        (0, col(255, 215, 70)),
        (1, col(90, 190, 255)),
        (2, col(190, 90, 230)),
        (3, col(240, 140, 65)),
        (4, col(215, 55, 90)),
        (5, col(75, 230, 190)),
        (6, col(110, 215, 110)),
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
    row = row.child(mk_pill_button(
        icon_label(Symbols::ROTATE_RIGHT, "F".into()),
        move || push_ui(&a_rot, UiAction::MakerRotateBrush),
    ));
    if st.selected_entity >= 5 {
        let a_ch = actions.clone();
        let label = if st.selected_entity == 5 {
            t(&st.translations, "toolbar-trigger", "Trigger Orb")
        } else {
            t(&st.translations, "toolbar-gate", "Relay Gate")
        };
        row = row.child(mk_pill_button(
            icon_label(
                Symbols::LINK,
                format!("{label} · Ch {} [L]", st.link_channel),
            ),
            move || push_ui(&a_ch, UiAction::MakerCycleLinkChannel),
        ));
    }
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

fn stepper_row(
    label: String,
    value: String,
    on_minus: impl Fn() + 'static,
    on_plus: impl Fn() + 'static,
) -> View {
    Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER)
        .gap(6.0))
    .children(vec![
        mk_icon_button(Symbols::REMOVE, true, on_minus),
        RText(format!("{label}: {value}"))
            .size(12.0)
            .color(col(200, 200, 210)),
        mk_icon_button(Symbols::ADD, true, on_plus),
    ])
}

fn inspector_panel(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let mut body: Vec<View> = vec![
        RText(t(tr, "inspector-title", "Inspector"))
            .size(12.0)
            .color(col(150, 150, 165)),
        spacer(6.0),
    ];

    if let Some(e) = &st.selected_entity_data {
        let (label_key, label_fb) = match e.kind {
            EntityKind::LaunchPad => ("maker-ent-pad", "Launch Pad"),
            EntityKind::Seal => ("maker-ent-seal", "Seal"),
            EntityKind::DriftPlate => ("maker-ent-drift", "Drift Plate"),
            EntityKind::Prowler => ("toolbar-prowler", "Prowler"),
            EntityKind::Glimmer => ("maker-ent-glimmer", "Glimmer"),
            EntityKind::TriggerOrb => ("toolbar-trigger", "Trigger Orb"),
            EntityKind::RelayGate => ("toolbar-gate", "Relay Gate"),
        };
        body.push(
            RText(t(tr, label_key, label_fb))
                .size(16.0)
                .color(RColor::WHITE),
        );
        body.push(
            RText(format!(
                "{} ({},{},{})",
                t(tr, "inspector-cell", "Cell"),
                e.cell[0],
                e.cell[1],
                e.cell[2]
            ))
            .size(12.0)
            .color(col(180, 180, 190)),
        );
        body.push(spacer(6.0));

        let (param_key, param_fb, step) = match e.kind {
            EntityKind::Glimmer => ("inspector-value", "Value", 0.5),
            EntityKind::LaunchPad => ("inspector-impulse", "Impulse", 0.5),
            EntityKind::Seal => ("inspector-glimmers", "Glimmers", 1.0),
            EntityKind::DriftPlate => ("inspector-period", "Period", 0.5),
            EntityKind::Prowler => ("inspector-speed", "Speed", 0.5),
            EntityKind::TriggerOrb => ("inspector-cooldown", "Cooldown", 0.5),
            EntityKind::RelayGate => ("inspector-duration", "Duration", 0.5),
        };
        let a_minus = actions.clone();
        let a_plus = actions.clone();
        body.push(stepper_row(
            t(tr, param_key, param_fb),
            format!("{:.1}", e.param),
            move || push_ui(&a_minus, UiAction::MakerInspParamDelta(-step)),
            move || push_ui(&a_plus, UiAction::MakerInspParamDelta(step)),
        ));

        if matches!(e.kind, EntityKind::TriggerOrb | EntityKind::RelayGate) {
            let a_minus = actions.clone();
            let a_plus = actions.clone();
            body.push(stepper_row(
                t(tr, "inspector-channel", "Channel"),
                format!("{}", e.link),
                move || push_ui(&a_minus, UiAction::MakerInspLinkDelta(-1)),
                move || push_ui(&a_plus, UiAction::MakerInspLinkDelta(1)),
            ));
        }

        if matches!(e.kind, EntityKind::LaunchPad | EntityKind::Prowler) {
            let a_minus = actions.clone();
            let a_plus = actions.clone();
            body.push(stepper_row(
                t(tr, "inspector-yaw", "Yaw"),
                format!("{}°", e.yaw_deg as i32),
                move || push_ui(&a_minus, UiAction::MakerInspYawDelta(-45.0)),
                move || push_ui(&a_plus, UiAction::MakerInspYawDelta(45.0)),
            ));
        }

        if matches!(e.kind, EntityKind::DriftPlate | EntityKind::Prowler) {
            let cur = e
                .track
                .map(|id| format!("#{id}"))
                .unwrap_or_else(|| t(tr, "inspector-none", "None"));
            let a_cycle = actions.clone();
            body.push(mk_pill_button(
                RText(format!("{}: {}", t(tr, "inspector-track", "Track"), cur)),
                move || push_ui(&a_cycle, UiAction::MakerInspTrackCycle),
            ));
        }

        body.push(spacer(6.0));
        let a_del = actions.clone();
        body.push(mk_pill_button(
            RText(t(tr, "inspector-delete", "Delete")),
            move || push_ui(&a_del, UiAction::MakerInspDeleteEntity),
        ));
    } else if let Some(track) = &st.active_track_data {
        body.push(
            RText(format!(
                "{} #{}",
                t(tr, "toolbar-tracks", "Tracks"),
                track.id
            ))
            .size(16.0)
            .color(RColor::WHITE),
        );
        body.push(
            RText(format!(
                "{}: {}",
                t(tr, "inspector-points", "Points"),
                track.points.len()
            ))
            .size(12.0)
            .color(col(180, 180, 190)),
        );
        body.push(spacer(6.0));

        let mode_label = match track.mode {
            TrackMode::PingPong => t(tr, "inspector-mode-pingpong", "PingPong"),
            TrackMode::Loop => t(tr, "inspector-mode-loop", "Loop"),
        };
        let a_mode = actions.clone();
        body.push(mk_pill_button(
            RText(format!(
                "{}: {}",
                t(tr, "inspector-mode", "Mode"),
                mode_label
            )),
            move || push_ui(&a_mode, UiAction::MakerInspTrackModeToggle),
        ));

        let a_minus = actions.clone();
        let a_plus = actions.clone();
        body.push(stepper_row(
            t(tr, "inspector-speed", "Speed"),
            format!("{:.1}", track.speed),
            move || push_ui(&a_minus, UiAction::MakerInspTrackSpeedDelta(-0.5)),
            move || push_ui(&a_plus, UiAction::MakerInspTrackSpeedDelta(0.5)),
        ));

        let a_rev = actions.clone();
        body.push(mk_pill_button(
            RText(t(tr, "inspector-reverse", "Reverse")),
            move || push_ui(&a_rev, UiAction::MakerInspTrackReverse),
        ));

        let a_del = actions.clone();
        body.push(mk_pill_button(
            RText(t(tr, "inspector-delete", "Delete")),
            move || push_ui(&a_del, UiAction::MakerInspTrackDelete),
        ));
    } else {
        body.push(
            RText(t(tr, "inspector-hint", "Select an entity or track"))
                .size(13.0)
                .color(col(180, 180, 190)),
        );
    }

    let mirror_label = match st.mirror {
        1 => "X",
        2 => "Z",
        3 => "X+Z",
        _ => "Off",
    };
    body.push(spacer(8.0));
    body.push(
        RText(format!(
            "{}: {}",
            t(tr, "inspector-mirror", "Mirror"),
            mirror_label
        ))
        .size(12.0)
        .color(col(180, 180, 190)),
    );

    Column(
        Modifier::new()
            .width(210.0)
            .padding(12.0)
            .background(RColor::from_rgba(15, 15, 22, 235))
            .clip_rounded(10.0)
            .align_items(AlignItems::FLEX_START),
    )
    .children(body)
}

// PLAY MODE

fn play_hud(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_edit = actions.clone();

    let mut children: Vec<View> = Vec::new();
    children.push(
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
                icon_text(
                    Symbols::TIMER,
                    format!("{:.1}s", st.play_time_secs),
                    16.0,
                    RColor::WHITE,
                ),
                icon_text(
                    Symbols::SKULL,
                    format!("{}", st.deaths),
                    16.0,
                    col(230, 120, 120),
                ),
                icon_text(
                    Symbols::AUTO_AWESOME,
                    format!("{}/{}", st.glimmers_collected, st.glimmers_total),
                    16.0,
                    col(255, 220, 110),
                ),
            )),
        ),
    );
    if !st.is_bundled {
        children.push(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .align_items(AlignItems::FLEX_END)
                    .justify_content(JustifyContent::FLEX_START)
                    .padding(10.0),
            )
            .child(mk_pill_button(
                icon_label(
                    Symbols::EDIT,
                    format!("{} [Tab]", t(tr, "toolbar-edit", "Edit")),
                ),
                move || push_ui(&a_edit, UiAction::MakerToggleMode),
            )),
        );
    }
    children.push(toast_anchor(st));

    ZStack(Modifier::new().fill_max_size()).children(children)
}

// Shared pieces

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

fn mk_icon_button(icon: Symbol, enabled: bool, on_click: impl Fn() + 'static) -> View {
    FilledTonalIconButton(
        Icon(icon).size(19.0),
        on_click,
        IconButtonConfig {
            enabled,
            container_size: Some(38.0),
            colors: IconButtonColors {
                container_color: col(60, 60, 80),
                content_color: RColor::WHITE,
                disabled_container_color: col(35, 35, 45),
                disabled_content_color: col(90, 90, 105),
            },
            ..Default::default()
        },
    )
}

fn icon_label(symbol: Symbol, text: String) -> View {
    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER))
        .child((Icon(symbol).size(16.0), RText(text)))
}

fn icon_text(symbol: Symbol, text: String, size: f32, color: RColor) -> View {
    Row(Modifier::new().gap(5.0).align_items(AlignItems::CENTER)).child((
        Icon(symbol).size(size).color(color),
        RText(text).size(size).color(color),
    ))
}

fn mk_pill_button(label: View, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .min_height(38.0)
            .padding(10.0)
            .clip_rounded(19.0),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}

fn mk_primary_button(label: View, bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new()
            .min_height(38.0)
            .padding(10.0)
            .background(bg)
            .clip_rounded(8.0),
        on_click,
        ButtonConfig::default(),
        move || label.clone(),
    )
}

fn level_clear_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_edit = actions.clone();
    let a_retry = actions.clone();
    let a_remix = actions.clone();
    let a_menu = actions.clone();

    let mut body: Vec<View> = vec![
        RText(t(tr, "maker-clear-title", "Level Clear!"))
            .size(38.0)
            .color(RColor::WHITE),
        spacer(12.0),
        RText(if st.level_verified {
            t(tr, "maker-clear-verified", "Level Verified!")
        } else {
            String::new()
        })
        .size(20.0)
        .color(col(90, 200, 120)),
        spacer(6.0),
        RText(format!(
            "{}: {:.2}s",
            t(tr, "maker-time", "Time"),
            st.clear_time_secs
        ))
        .size(18.0)
        .color(RColor::WHITE),
    ];

    if st.player_is_author {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{} ({:.2}s)",
                t(tr, "maker-cleared", "Cleared!"),
                st.clear_time_secs
            ),
            16.0,
            col(120, 230, 140),
        ));
    } else if st.new_record {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{} ({:.2}s)",
                t(tr, "maker-new-record", "New record!"),
                st.clear_time_secs
            ),
            16.0,
            col(120, 230, 140),
        ));
    } else if let Some(record) = st.record_ms {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{}: {:.2}s",
                t(tr, "maker-record", "Record"),
                record as f32 / 1000.0
            ),
            16.0,
            col(255, 200, 90),
        ));
    } else if st.first_clear {
        body.push(icon_text(
            Symbols::STAR,
            format!(
                "{} ({:.2}s)",
                t(tr, "maker-first-clear", "First clear!"),
                st.clear_time_secs
            ),
            16.0,
            col(120, 230, 140),
        ));
    }

    body.push(
        RText(format!(
            "{}: {}",
            t(tr, "maker-deaths", "Deaths"),
            st.clear_deaths
        ))
        .size(18.0)
        .color(RColor::WHITE),
    );
    body.push(
        RText(format!(
            "{}: {}/{}",
            t(tr, "maker-glimmers-count", "Glimmers"),
            st.glimmers_collected,
            st.glimmers_total
        ))
        .size(18.0)
        .color(col(255, 220, 100)),
    );
    body.push(
        RText(format!(
            "{}: {}",
            t(tr, "maker-blocks-count", "Blocks"),
            st.blocks_placed
        ))
        .size(18.0)
        .color(RColor::WHITE),
    );

    body.push(spacer(16.0));
    // Bundled levels are read-only sources: Remix instead of editing in place.
    if st.is_bundled {
        body.push(mk_button(
            &t(tr, "maker-remix", "Remix This Level"),
            col(150, 100, 220),
            move || push_ui(&a_remix, UiAction::MakerRemix),
        ));
    } else {
        body.push(mk_button(
            &t(tr, "maker-btn-edit", "Edit Level"),
            col(70, 110, 170),
            move || push_ui(&a_edit, UiAction::MakerDismissClear),
        ));
    }
    body.push(mk_button(
        &t(tr, "maker-retry", "Retry"),
        col(60, 140, 90),
        move || push_ui(&a_retry, UiAction::MakerRetry),
    ));
    body.push(mk_button(
        &t(tr, "back-to-menu", "Back to Menu"),
        col(180, 60, 60),
        move || push_ui(&a_menu, UiAction::QuitToTitle),
    ));

    let inner = Column(
        Modifier::new()
            .width(380.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .children(body);

    modal_shell(inner)
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

    modal_shell(inner)
}

fn browse_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_close = actions.clone();
    let a_sort = actions.clone();
    let a_clear = actions.clone();
    let a_query = actions.clone();

    let levels = &st.browse_visible;

    let header = Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER))
    .child((
        RText("My Levels").size(28.0).color(RColor::WHITE),
        Column(Modifier::new().fill_max_width()),
        mk_icon_button(Symbols::REMOVE, true, move || {
            push(&a_close, UiAction::CloseOverlay)
        }),
    ));

    let query_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let query_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !query_focus.get() && query_state.borrow().text != st.browse_query {
        query_state.borrow_mut().text = st.browse_query.clone();
    }
    let on_change = {
        let a = a_query.clone();
        Rc::new(move |v: String| push(&a, UiAction::BrowseSetQuery(v))) as Rc<dyn Fn(String)>
    };
    let field = BasicTextField(
        query_state.clone(),
        Modifier::new().flex_grow(1.0).height(34.0),
        "Search name, author, description",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Filter,
                ime_action: ImeAction::Search,
                ..KeyboardOptions::DEFAULT
            },
            on_change: Some(on_change),
            focus_tracker: Some(query_focus.clone()),
            ..Default::default()
        },
    );

    let mut search_children: Vec<View> = vec![
        Icon(Symbols::SEARCH).size(18.0).color(col(150, 150, 170)),
        field,
    ];
    if !st.browse_query.is_empty() {
        let qs = query_state.clone();
        let a_clear2 = a_clear.clone();
        search_children.push(mk_icon_button(Symbols::REMOVE, true, move || {
            qs.borrow_mut().text.clear();
            push(&a_clear2, UiAction::BrowseClearQuery)
        }));
    }
    let search_row = Row(Modifier::new()
        .fill_max_width()
        .gap(8.0)
        .align_items(AlignItems::CENTER)
        .padding(10.0)
        .background(col(45, 45, 60))
        .clip_rounded(18.0))
    .child(search_children);

    let hint =
        RText("Try: name:air  author:mlm  tag:puzzle  #precision  verified:1  diff:hard  has:gate")
            .size(11.0)
            .color(col(130, 130, 150));

    // Verified + difficulty chips (filters, not level tags).
    let verified_label = if st.browse_verified_only {
        icon_label(Symbols::CHECK, "Verified".into())
    } else {
        RText("Verified").size(12.0).color(col(150, 150, 170))
    };
    let a_ver = actions.clone();
    let mut filter_chips: Vec<View> = vec![mk_chip(
        verified_label,
        st.browse_verified_only,
        col(90, 200, 120),
        move || push(&a_ver, UiAction::BrowseToggleVerified),
    )];
    for i in 0..4u8 {
        let a = actions.clone();
        let selected = st.browse_difficulty == Some(i);
        let label = RText(difficulty_label(i)).size(12.0).color(if selected {
            RColor::WHITE
        } else {
            col(150, 150, 170)
        });
        let diff_color = match i {
            0 => col(100, 180, 140),
            1 => col(150, 170, 90),
            2 => col(220, 150, 90),
            _ => col(220, 90, 90),
        };
        filter_chips.push(mk_chip(label, selected, diff_color, move || {
            push(
                &a,
                UiAction::BrowseSetDifficulty(if selected { None } else { Some(i) }),
            )
        }));
    }
    let filter_row = Row(Modifier::new().gap(6.0)).child(filter_chips);

    // Tag include chips.
    let mut chip_views: Vec<View> = Vec::new();
    for tag in LevelTag::ALL {
        let a = actions.clone();
        let included = st.browse_include_tags.contains(&tag);
        let label = RText(tag.label()).size(12.0).color(if included {
            RColor::WHITE
        } else {
            col(150, 150, 170)
        });
        chip_views.push(mk_chip(label, included, tag_color(tag), move || {
            push(&a, UiAction::BrowseToggleTag(tag))
        }));
    }
    let tag_row = Row(Modifier::new().gap(6.0)).child(chip_views);

    let sort_label = match st.browse_sort % 6 {
        0 => "Sort: Recent",
        1 => "Sort: Name",
        2 => "Sort: Shortest",
        3 => "Sort: Longest",
        4 => "Sort: Fastest clear",
        _ => "Sort: Hardest",
    };
    let sort_text = RText(sort_label).size(13.0).color(col(190, 190, 205));
    let sort_button = FilledTonalButton(
        Modifier::new()
            .height(30.0)
            .padding(12.0)
            .background(col(45, 45, 60))
            .clip_rounded(15.0),
        move || push(&a_sort, UiAction::BrowseCycleSort),
        ButtonConfig::default(),
        move || sort_text.clone(),
    );
    let count_text = RText(format!("{} levels", levels.len()))
        .size(12.0)
        .color(col(150, 150, 170));
    let sort_row = Row(Modifier::new().gap(12.0).align_items(AlignItems::CENTER))
        .child((sort_button, count_text));

    let mut grid_children: Vec<View> = Vec::new();
    if st.browse_levels.is_empty() {
        grid_children.push(
            RText("Import a share code or save to your collection from Share.")
                .size(14.0)
                .color(col(180, 180, 190)),
        );
    } else if levels.is_empty() {
        grid_children.push(
            RText("No matches. Try author:, tag:puzzle, has:gate, verified:1.")
                .size(14.0)
                .color(col(180, 180, 190)),
        );
    } else {
        for s in levels {
            grid_children.push(browse_card(s, st, &actions));
        }
    }

    let scroll_state = remember_scroll_state("browse_list");
    let grid_view = repose_ui::Grid(
        3,
        Modifier::new().fill_max_width(),
        grid_children,
        10.0,
        10.0,
    );
    let scroll_list = ScrollArea(
        Modifier::new().fill_max_width().weight(1.0),
        scroll_state,
        grid_view,
    );

    // Detail panel: identity + actions, rendered when a card is selected.
    let detail: View = match st
        .browse_visible
        .iter()
        .find(|s| st.browse_selected.as_deref() == Some(s.key.as_str()))
    {
        Some(sd) => local_detail_panel(sd, st, &actions),
        None => BrowseSelectPanel(vec![
            RText("Select a level").size(14.0).color(col(150, 150, 170)),
        ]),
    };

    let inner = Column(
        Modifier::new()
            .fill_max_width()
            .width(980.0)
            .max_height(760.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(header)
    .child(spacer(12.0))
    .child(search_row)
    .child(spacer(6.0))
    .child(hint)
    .child(spacer(10.0))
    .child(filter_row)
    .child(spacer(8.0))
    .child(tag_row)
    .child(spacer(10.0))
    .child(sort_row)
    .child(spacer(12.0))
    .child(
        Row(Modifier::new().fill_max_width().align_items(AlignItems::CENTER)).child((
            Column(Modifier::new().fill_max_width().weight(1.0)).child(scroll_list),
            detail,
        )),
    );

    modal_shell(inner)
}

fn diff_strip_color(difficulty: u8) -> RColor {
    match difficulty {
        0 => col(100, 180, 140),
        1 => col(150, 170, 90),
        2 => col(220, 150, 90),
        _ => col(220, 90, 90),
    }
}

fn browse_card(
    s: &LevelSummary,
    st: &SharedUi,
    actions: &Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let b_sel = actions.clone();
    let k = s.key.clone();
    let selected = st.browse_selected.as_deref() == Some(s.key.as_str());

    let mut name_children: Vec<View> =
        vec![RText(s.name.clone()).size(16.0).color(RColor::WHITE)];
    if s.verified {
        name_children.push(Icon(Symbols::CHECK).size(15.0).color(col(220, 210, 120)));
    }
    if s.source == LevelSourceKind::Collection {
        name_children.push(
            Icon(Symbols::FOLDER_OPEN).size(14.0).color(col(150, 150, 170)),
        );
    }

    let card_config = CardConfig {
        border: Some((if selected { 2.0 } else { 1.0 }, if selected {
            col(255, 217, 59)
        } else {
            RColor::from_rgba(255, 255, 255, 40)
        })),
        shape_radius: 12.0,
        ..Default::default()
    };

    ClickableOutlinedCard(
        move || push(&b_sel, UiAction::BrowseSelect(k.clone())),
        Modifier::new().fill_max_width(),
        card_config,
        move || {
            Column(Modifier::new().gap(6.0).align_items(AlignItems::FLEX_START)).child((
                // Thumbnail as the card's face (identity only).
                Column(
                    Modifier::new()
                        .fill_max_width()
                        .align_items(AlignItems::CENTER)
                        .justify_content(JustifyContent::CENTER)
                        .background(col(30, 30, 42))
                        .clip_rounded(8.0)
                        .height(96.0),
                )
                .child(thumb_grid_view(&s.preview)),
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER))
                    .child(name_children),
                Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child((
                    Column(
                        Modifier::new()
                            .width(46.0)
                            .height(18.0)
                            .background(diff_strip_color(s.difficulty))
                            .clip_rounded(9.0)
                            .align_items(AlignItems::CENTER),
                    )
                    .child(
                        RText(difficulty_label(s.difficulty))
                            .size(11.0)
                            .color(RColor::from_rgba(0, 0, 0, 230)),
                    ),
                    RText(
                        if s.author.is_empty() {
                            "Unknown".to_string()
                        } else {
                            s.author.clone()
                        },
                    )
                    .size(12.0)
                    .color(col(150, 150, 170)),
                )),
            ))
        },
    )
}

fn local_detail_panel(
    s: &LevelSummary,
    st: &SharedUi,
    actions: &Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let a_play = actions.clone();
    let a_edit = actions.clone();
    let a_del = actions.clone();
    let k_play = s.key.clone();
    let k_edit = s.key.clone();
    let k_del = s.key.clone();

    let mut name_children: Vec<View> =
        vec![RText(s.name.clone()).size(20.0).color(RColor::WHITE)];
    if s.verified {
        name_children.push(Icon(Symbols::CHECK).size(16.0).color(col(220, 210, 120)));
    }

    let mut tag_pills: Vec<View> = Vec::new();
    for tag in &s.tags {
        tag_pills.push(
            Column(
                Modifier::new()
                    .padding(6.0)
                    .background(tag_color(*tag))
                    .clip_rounded(8.0),
            )
            .child(RText(tag.label()).size(11.0).color(col(170, 170, 190))),
        );
    }

    let mut stats = format!("{} blocks · {} ents", s.block_count, s.entity_count);
    if s.track_count > 0 {
        stats.push_str(&format!(" · {} tracks", s.track_count));
    }
    tag_pills.push(RText(stats).size(11.0).color(col(140, 140, 160)));

    let confirming_delete = st.browse_confirm_delete.as_deref() == Some(s.key.as_str());

    let action_row: View = if confirming_delete {
        let a_confirm = actions.clone();
        let a_cancel = actions.clone();
        let k_confirm = s.key.clone();

        Column(Modifier::new().gap(6.0)).child((
            RText("Delete this level?")
                .size(12.0)
                .color(col(232, 120, 120)),
            Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                FilledTonalButton(
                    Modifier::new()
                        .height(34.0)
                        .background(col(160, 60, 60))
                        .clip_rounded(8.0),
                    move || push(&a_confirm, UiAction::BrowseConfirmDelete(k_confirm.clone())),
                    ButtonConfig::default(),
                    move || RText("Confirm Delete").size(13.0).color(RColor::WHITE),
                ),
                FilledTonalButton(
                    Modifier::new().height(34.0),
                    move || push(&a_cancel, UiAction::BrowseCancelDelete),
                    ButtonConfig::default(),
                    move || RText("Cancel").size(13.0).color(RColor::WHITE),
                ),
            )),
        ))
    } else {
        Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_play, UiAction::BrowsePlay(k_play.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::PLAY_ARROW).size(18.0).color(RColor::WHITE),
                        RText("Play").size(14.0).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_edit, UiAction::BrowseEdit(k_edit.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::EDIT).size(16.0).color(RColor::WHITE),
                        RText("Edit").size(14.0).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_del, UiAction::BrowseDelete(k_del.clone())),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::DELETE).size(16.0).color(col(232, 120, 120)),
                        RText("Delete").size(14.0).color(col(232, 120, 120)),
                    ))
                },
            ),
        ))
    };

    Column(
        Modifier::new()
            .width(280.0)
            .fill_max_height()
            .background(col(24, 24, 34))
            .clip_rounded(12.0)
            .padding(14.0)
            .gap(8.0),
    )
    .child((
        // Large preview
        Column(
            Modifier::new()
                .fill_max_width()
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER)
                .background(col(30, 30, 42))
                .clip_rounded(8.0)
                .height(120.0),
        )
        .child(thumb_grid_view(&s.preview)),
        Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child(name_children),
        RText(
            if s.author.is_empty() {
                "Unknown".to_string()
            } else {
                s.author.clone()
            },
        )
        .size(12.0)
        .color(col(150, 150, 170)),
        Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child(tag_pills),
        action_row,
    ))
}

/// Right-side detail panel shown before any card is selected.
#[allow(non_snake_case)]
fn BrowseSelectPanel(views: Vec<View>) -> View {
    Column(
        Modifier::new()
            .width(280.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
    )
    .with_children(views)
}

fn online_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_close = actions.clone();
    let a_refresh = actions.clone();
    let a_clear = actions.clone();
    let a_submit_search = actions.clone();
    let a_submit_token = actions.clone();
    let a_search = actions.clone();
    let a_set_token = actions.clone();
    let a_upload = actions.clone();
    let a_sort = actions.clone();

    let levels = &st.online_levels;

    let header = Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER))
    .child((
        RText("Online Levels").size(28.0).color(RColor::WHITE),
        Column(Modifier::new().fill_max_width()),
        mk_icon_button(Symbols::REFRESH, true, move || {
            push(&a_refresh, UiAction::OnlineRefresh)
        }),
        mk_icon_button(Symbols::REMOVE, true, move || {
            push(&a_close, UiAction::CloseOverlay)
        }),
    ));

    let query_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let query_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !query_focus.get() && query_state.borrow().text != st.online_query {
        query_state.borrow_mut().text = st.online_query.clone();
    }
    let on_change = {
        let a = actions.clone();
        Rc::new(move |v: String| push(&a, UiAction::OnlineSetQuery(v))) as Rc<dyn Fn(String)>
    };
    let on_submit = {
        let a = a_submit_search.clone();
        Rc::new(move |v: String| {
            push(&a, UiAction::OnlineSetQuery(v));
            push(&a, UiAction::OnlineSearch);
        }) as Rc<dyn Fn(String)>
    };
    let search_field = BasicTextField(
        query_state.clone(),
        Modifier::new().flex_grow(1.0).height(34.0),
        "Search name or author",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Filter,
                ime_action: ImeAction::Search,
                ..KeyboardOptions::DEFAULT
            },
            on_change: Some(on_change),
            on_submit: Some(on_submit),
            focus_tracker: Some(query_focus.clone()),
            ..Default::default()
        },
    );

    let mut search_children: Vec<View> = vec![
        Icon(Symbols::SEARCH).size(18.0).color(col(150, 150, 170)),
        search_field,
    ];
    if !st.online_query.is_empty() {
        let qs = query_state.clone();
        let a_clear2 = a_clear.clone();
        search_children.push(mk_icon_button(Symbols::REMOVE, true, move || {
            qs.borrow_mut().text.clear();
            push(&a_clear2, UiAction::OnlineClearQuery)
        }));
    }
    let search_row = Row(Modifier::new()
        .fill_max_width()
        .gap(8.0)
        .align_items(AlignItems::CENTER))
    .child((
        Row(Modifier::new()
            .fill_max_width()
            .gap(8.0)
            .align_items(AlignItems::CENTER)
            .padding(10.0)
            .background(col(45, 45, 60))
            .clip_rounded(18.0))
        .child(search_children),
        mk_pill_button(icon_label(Symbols::SEARCH, "Search".into()), move || {
            let q = query_state.borrow().text.clone();
            push(&a_search, UiAction::OnlineSetQuery(q));
            push(&a_search, UiAction::OnlineSearch);
        }),
    ));

    let token_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let token_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !token_focus.get() && token_state.borrow().text != st.online_token {
        token_state.borrow_mut().text = st.online_token.clone();
    }
    let on_token_submit = {
        let a = a_submit_token.clone();
        Rc::new(move |v: String| push(&a, UiAction::OnlineSetToken(v))) as Rc<dyn Fn(String)>
    };
    let token_field = BasicTextField(
        token_state.clone(),
        Modifier::new().flex_grow(1.0).height(34.0),
        "Upload token (needed to publish / delete)",
        TextFieldConfig {
            line_limits: TextFieldLineLimits::SingleLine,
            keyboard_options: KeyboardOptions {
                keyboard_type: KeyboardType::Text,
                ime_action: ImeAction::Done,
                ..KeyboardOptions::DEFAULT
            },
            on_submit: Some(on_token_submit),
            focus_tracker: Some(token_focus.clone()),
            ..Default::default()
        },
    );
    let token_row = Row(Modifier::new()
        .fill_max_width()
        .gap(8.0)
        .align_items(AlignItems::CENTER))
    .child((
        Row(Modifier::new()
            .fill_max_width()
            .gap(8.0)
            .align_items(AlignItems::CENTER)
            .padding(10.0)
            .background(col(45, 45, 60))
            .clip_rounded(18.0))
        .child((
            Icon(Symbols::LINK).size(16.0).color(col(150, 150, 170)),
            token_field,
        )),
        mk_pill_button(icon_label(Symbols::CHECK, "Set".into()), move || {
            let t = token_state.borrow().text.clone();
            push(&a_set_token, UiAction::OnlineSetToken(t));
        }),
    ));

    let verified_hint = RText(if st.level_verified {
        "Ready to publish".to_string()
    } else {
        "Beat the level to publish it".to_string()
    })
    .size(12.0)
    .color(if st.level_verified {
        col(90, 200, 120)
    } else {
        col(230, 160, 70)
    });

    let upload_button = mk_primary_button(
        icon_label(Symbols::CLOUD_UPLOAD, "Publish Current Level".into()),
        col(150, 110, 200),
        move || push(&a_upload, UiAction::OnlineUpload),
    );

    let sort_label = match st.online_sort % 4 {
        0 => "Sort: Newest",
        1 => "Sort: Name",
        2 => "Sort: Most liked",
        _ => "Sort: Most played",
    };
    let sort_text = RText(sort_label).size(13.0).color(col(190, 190, 205));
    let sort_button = FilledTonalButton(
        Modifier::new()
            .height(30.0)
            .padding(12.0)
            .background(col(45, 45, 60))
            .clip_rounded(15.0),
        move || push(&a_sort, UiAction::OnlineCycleSort),
        ButtonConfig::default(),
        move || sort_text.clone(),
    );
    let count_text: View = if st.online_loading {
        icon_text(
            Symbols::REFRESH,
            "Loading...".into(),
            12.0,
            col(230, 160, 70),
        )
    } else {
        RText(format!("{} levels", levels.len()))
            .size(12.0)
            .color(col(150, 150, 170))
    };
    let _sort_row: ();

    // Shelf tabs (MM2 category shelves): New / Popular / Hot.
    let shelf_labels = ["Fresh", "Popular", "Hot"];
    let mut shelf_row_children: Vec<View> = Vec::new();
    for (i, name) in shelf_labels.iter().enumerate() {
        let a = actions.clone();
        let idx = i as u8;
        let selected = st.online_shelf == idx;
        shelf_row_children.push(InputChip(
            selected,
            move || push(&a, UiAction::OnlineSetShelf(idx)),
            RText(*name).size(13.0).color(if selected {
                RColor::WHITE
            } else {
                col(160, 160, 175)
            }),
            None,
            None,
            None,
            ChipConfig {
                shape_radius: 10.0,
                ..Default::default()
            },
        ));
    }
    let shelf_row = Row(Modifier::new().gap(6.0)).child(shelf_row_children);

    // Dedicated "Search with ID" field (always visible, MM2 pattern).
    let id_state: Rc<RefCell<TextFieldState>> = remember(|| RefCell::new(TextFieldState::new()));
    let id_focus: Rc<Cell<bool>> = remember(|| Cell::new(false));
    if !id_focus.get() && id_state.borrow().text != st.online_id_query {
        id_state.borrow_mut().text = st.online_id_query.clone();
    }
    let id_change = actions.clone();
    let id_submit = actions.clone();
    let a_id_go = actions.clone();
    let id_field = OutlinedTextField(
        Modifier::new().width(170.0),
        st.online_id_query.clone(),
        move |v: String| push(&id_change, UiAction::OnlineSetIdQuery(v)),
        OutlinedTextFieldConfig {
            label: Some("Level ID".into()),
            placeholder: None,
            single_line: true,
            on_submit: Some(Rc::new(move |_v: String| push(&id_submit, UiAction::OnlineSearchId))),
            ..Default::default()
        },
    );
    let id_row = Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
        id_field,
        FilledTonalButton(
            Modifier::new().height(36.0),
            move || push(&a_id_go, UiAction::OnlineSearchId),
            ButtonConfig::default(),
            move || {
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                    Icon(Symbols::SEARCH).size(16.0).color(RColor::WHITE),
                    RText("Go").size(13.0).color(RColor::WHITE),
                ))
            },
        ),
    ));

    // A 3-across grid of identity-only cards.
    let mut grid_children: Vec<View> = Vec::new();
    if levels.is_empty() {
        grid_children.push(
            RText("No levels online yet. Publish one, or search / refresh to re-fetch.")
                .size(14.0)
                .color(col(180, 180, 190)),
        );
    } else {
        for m in levels {
            grid_children.push(online_card(m, st, &actions));
        }
    }

    let scroll_state = remember_scroll_state("online_list");
    let grid_view = repose_ui::Grid(3, Modifier::new().fill_max_width(), grid_children, 10.0, 10.0);
    let scroll_list = ScrollArea(
        Modifier::new().fill_max_width().weight(1.0),
        scroll_state,
        grid_view,
    );

    // Detail panel: identity + actions, shown when a card is selected.
    let detail: View = match st
        .online_levels
        .iter()
        .find(|m| st.online_selected == Some(m.id))
    {
        Some(m) => online_detail_panel(m, st, &actions),
        None => BrowseSelectPanel(vec![
            RText("Select a level").size(14.0).color(col(150, 150, 170)),
        ]),
    };

    let inner = Column(
        Modifier::new()
            .fill_max_width()
            .width(980.0)
            .max_height(760.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(header)
    .child(spacer(12.0))
    .child(search_row)
    .child(spacer(8.0))
    .child(token_row)
    .child(spacer(12.0))
    .child(
        Row(Modifier::new().gap(12.0).align_items(AlignItems::CENTER)).child((
            upload_button,
            verified_hint,
            Column(Modifier::new().fill_max_width()),
            id_row,
        )),
    )
    .child(spacer(12.0))
    .child(
        Row(Modifier::new().gap(12.0).align_items(AlignItems::CENTER)).child((
            shelf_row,
            Column(Modifier::new().fill_max_width()),
            sort_button,
            count_text,
        )),
    )
    .child(spacer(12.0))
    .child(
        Row(Modifier::new().fill_max_width().align_items(AlignItems::CENTER)).child((
            Column(Modifier::new().fill_max_width().weight(1.0)).child(scroll_list),
            detail,
        )),
    );

    modal_shell(inner)
}

/// An identity-only online card: generated preview when available, name, author,
/// stats. No actions on the tile; click selects it and opens the detail panel.
fn online_card(m: &LevelMeta, st: &SharedUi, actions: &Arc<Mutex<Vec<UiAction>>>) -> View {
    let b_sel = actions.clone();
    let a_preview = actions.clone();
    let id = m.id;
    let selected = st.online_selected == Some(m.id);

    // Lazy-preview request. This is intentionally data-driven, not stored
    // thumbnail-driven: download level data once, generate ThumbPreview locally.
    let needs_preview =
        !st.online_previews.contains_key(&m.id) && !st.online_preview_pending.contains(&m.id);
    if needs_preview {
        push(&a_preview, UiAction::OnlinePreview(m.id));
    }

    let preview = st.online_previews.get(&m.id).cloned();
    let pending = st.online_preview_pending.contains(&m.id);

    let card_config = CardConfig {
        border: Some((
            if selected { 2.0 } else { 1.0 },
            if selected {
                col(255, 217, 59)
            } else {
                RColor::from_rgba(255, 255, 255, 40)
            },
        )),
        shape_radius: 12.0,
        ..Default::default()
    };

    ClickableOutlinedCard(
        move || push(&b_sel, UiAction::OnlineSelect(id)),
        Modifier::new().fill_max_width(),
        card_config,
        move || {
            let date: String = m.created_at.chars().take(10).collect();

            let preview_view: View = match preview {
                Some(p) => Column(
                    Modifier::new()
                        .fill_max_width()
                        .height(92.0)
                        .align_items(AlignItems::CENTER)
                        .justify_content(JustifyContent::CENTER)
                        .background(col(30, 30, 42))
                        .clip_rounded(8.0),
                )
                .child(thumb_grid_view(&p)),
                None => online_preview_placeholder(m.id, pending),
            };

            let children: Vec<View> = vec![
                preview_view,
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                    RText(m.name.clone()).size(15.0).color(RColor::WHITE),
                    RText(format!("#{}", m.id)).size(11.0).color(col(130, 130, 150)),
                )),
                RText(
                    if m.author.is_empty() {
                        "Unknown".to_string()
                    } else {
                        m.author.clone()
                    },
                )
                .size(12.0)
                .color(col(150, 150, 170)),
                RText(format!("♥ {}  ▶ {} · {}", m.likes, m.plays, date))
                    .size(11.0)
                    .color(col(140, 140, 160)),
            ];

            Column(Modifier::new().gap(6.0).align_items(AlignItems::FLEX_START))
                .with_children(children)
        },
    )
}

/// Placeholder shown while an online card's preview is not available yet.
fn online_preview_placeholder(id: u64, pending: bool) -> View {
    let label = if pending {
        "Generating preview…".to_string()
    } else {
        format!("#{id}")
    };

    Column(
        Modifier::new()
            .fill_max_width()
            .height(92.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .background(col(30, 30, 42))
            .clip_rounded(8.0),
    )
    .child((
        Icon(Symbols::CLOUD).size(22.0).color(col(120, 120, 145)),
        spacer(4.0),
        RText(label).size(12.0).color(col(140, 140, 160)),
    ))
}

/// Right-side detail panel for a selected online level with its actions.
fn online_detail_panel(
    m: &LevelMeta,
    st: &SharedUi,
    actions: &Arc<Mutex<Vec<UiAction>>>,
) -> View {
    let a_play = actions.clone();
    let a_like = actions.clone();
    let a_delete = actions.clone();
    let a_report = actions.clone();
    let id = m.id;

    let action_row: View = Column(Modifier::new().gap(6.0)).child((
        FilledTonalButton(
            Modifier::new().height(36.0).fill_max_width(),
            move || push(&a_play, UiAction::OnlinePlay(id)),
            ButtonConfig::default(),
            move || {
                Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                    Icon(Symbols::PLAY_ARROW).size(18.0).color(RColor::WHITE),
                    RText("Play").size(14.0).color(RColor::WHITE),
                ))
            },
        ),
        Row(Modifier::new().gap(6.0)).child((
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_like, UiAction::OnlineLike(id)),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::THUMB_UP).size(16.0).color(RColor::WHITE),
                        RText("Like").size(13.0).color(RColor::WHITE),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_delete, UiAction::OnlineDelete(id)),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::DELETE).size(16.0).color(col(232, 120, 120)),
                        RText("Delete").size(13.0).color(col(232, 120, 120)),
                    ))
                },
            ),
            FilledTonalButton(
                Modifier::new().height(34.0),
                move || push(&a_report, UiAction::OnlineReport(id)),
                ButtonConfig::default(),
                move || {
                    Row(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child((
                        Icon(Symbols::FLAG).size(16.0).color(RColor::WHITE),
                        RText("Report").size(13.0).color(RColor::WHITE),
                    ))
                },
            ),
        )),
    ));

    let mut tag_pills: Vec<View> = Vec::new();
    for tag in &m.tags {
        tag_pills.push(
            Column(Modifier::new().padding(6.0).background(col(60, 60, 80)).clip_rounded(8.0))
                .child(RText(tag.clone()).size(11.0).color(col(170, 170, 190))),
        );
    }

    Column(
        Modifier::new()
            .width(280.0)
            .fill_max_height()
            .background(col(24, 24, 34))
            .clip_rounded(12.0)
            .padding(14.0)
            .gap(8.0),
    )
    .child((
        match st.online_previews.get(&m.id) {
            Some(p) => Column(
                Modifier::new()
                    .fill_max_width()
                    .height(120.0)
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::CENTER)
                    .background(col(30, 30, 42))
                    .clip_rounded(8.0),
            )
            .child(thumb_grid_view(p)),
            None => online_preview_placeholder(m.id, st.online_preview_pending.contains(&m.id)),
        },
        RText(m.name.clone()).size(19.0).color(RColor::WHITE),
        RText(
            if m.author.is_empty() {
                "Unknown".to_string()
            } else {
                m.author.clone()
            },
        )
        .size(13.0)
        .color(col(150, 150, 170)),
        if m.tags.is_empty() {
            RText("").size(0.0)
        } else {
            Row(Modifier::new().gap(6.0)).child(tag_pills)
        },
        RText(format!(
            "{} likes · {} plays · {:.1} KB",
            m.likes,
            m.plays,
            m.size_bytes as f32 / 1024.0
        ))
        .size(12.0)
        .color(col(140, 140, 160)),
        action_row,
    ))
}

/// Renders an isometric preview as a grid of colored boxes (Repose primitives
/// only).
fn thumb_grid_view(p: &ThumbPreview) -> View {
    const CELL: f32 = 4.0;

    let mut rows: Vec<View> = Vec::with_capacity(p.rows);
    for r in 0..p.rows {
        let mut cells: Vec<View> = Vec::with_capacity(p.cols);
        for cidx in 0..p.cols {
            let px = p.cells[r * p.cols + cidx];
            cells.push(Column(
                Modifier::new()
                    .width(CELL)
                    .height(CELL)
                    .background(RColor::from_rgba(px[0], px[1], px[2], px[3])),
            ));
        }
        rows.push(Row(Modifier::new()).child(cells));
    }

    Column(
        Modifier::new()
            .clip_rounded(8.0)
            .background(col(30, 30, 40)),
    )
    .child(rows)
}

fn tag_color(tag: LevelTag) -> RColor {
    match tag {
        LevelTag::Short => col(80, 150, 210),
        LevelTag::Puzzle => col(150, 110, 220),
        LevelTag::Precision => col(220, 110, 110),
        LevelTag::Chill => col(100, 180, 140),
        LevelTag::Music => col(220, 170, 100),
        LevelTag::Auto => col(120, 180, 200),
    }
}

fn mk_chip(label: View, selected: bool, color: RColor, on_click: impl Fn() + 'static) -> View {
    let bg = if selected { color } else { col(45, 45, 60) };
    let mut cfg = ButtonConfig::default();
    cfg.state_elevation = Some(StateElevation {
        default: 0.0,
        hovered: 0.0,
        pressed: 0.0,
        disabled: 0.0,
    });
    cfg.state_colors = StateColors {
        default: RColor::from_rgba(0, 0, 0, 0),
        hovered: RColor::from_rgba(255, 255, 255, 16),
        pressed: RColor::from_rgba(255, 255, 255, 28),
        disabled: RColor::from_rgba(0, 0, 0, 0),
    };
    FilledTonalButton(
        Modifier::new()
            .height(30.0)
            .padding(10.0)
            .background(bg)
            .clip_rounded(15.0),
        on_click,
        cfg,
        move || label.clone(),
    )
}

fn share_overlay_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let tr = &st.translations;
    let a_close = actions.clone();
    let a_copy = actions.clone();
    let a_export = actions.clone();
    let a_import = actions.clone();
    let a_save = actions.clone();
    let import_code = st.import_code.clone();

    let mut tail: Vec<View> = Vec::new();
    if !st.is_bundled {
        tail.push(mk_primary_button(
            RText("Save to My Collection"),
            col(150, 110, 200),
            move || push(&a_save, UiAction::BrowseAddToCollection),
        ));
    }
    tail.push(spacer(16.0));
    tail.push(mk_button(
        &t(tr, "back", "Back"),
        col(70, 70, 90),
        move || push_ui(&a_close, UiAction::CloseOverlay),
    ));

    let inner = Column(
        Modifier::new()
            .width(520.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(
        RText(t(tr, "share-title", "Share Level"))
            .size(32.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.level_verified {
            t(tr, "share-verified", "Verified")
        } else {
            t(tr, "share-unverified", "Beat the level to share it")
        })
        .size(14.0)
        .color(if st.level_verified {
            col(90, 200, 120)
        } else {
            col(230, 160, 70)
        }),
    )
    .child(spacer(16.0))
    .child(
        RText(t(tr, "share-export-title", "Export"))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.export_code.is_empty() {
            t(tr, "share-export-empty", "No code yet")
        } else {
            st.export_code.clone()
        })
        .size(14.0)
        .color(if st.export_code.is_empty() {
            col(180, 180, 190)
        } else {
            col(120, 200, 255)
        }),
    )
    .child(spacer(4.0))
    .child(
        RText(st.export_error.clone().unwrap_or_default())
            .size(13.0)
            .color(col(230, 110, 110)),
    )
    .child(spacer(8.0))
    .child(mk_button(
        &t(tr, "share-export", "Generate Code"),
        col(70, 110, 170),
        move || push_ui(&a_export, UiAction::MakerExportCode),
    ))
    .child(mk_button(
        &t(tr, "share-copy", "Copy Code"),
        col(60, 140, 90),
        move || push_ui(&a_copy, UiAction::MakerCopyCode),
    ))
    .child(spacer(20.0))
    .child(
        RText(t(tr, "share-import-title", "Import"))
            .size(18.0)
            .color(RColor::WHITE),
    )
    .child(spacer(8.0))
    .child(
        RText(if st.import_code.is_empty() {
            t(
                tr,
                "share-import-hint",
                "Type or paste a code, then press Enter",
            )
        } else {
            st.import_code.clone()
        })
        .size(14.0)
        .color(if st.import_code.is_empty() {
            col(180, 180, 190)
        } else {
            col(120, 200, 255)
        }),
    )
    .child(spacer(8.0))
    .child(mk_button(
        &t(tr, "share-import", "Import"),
        col(160, 120, 60),
        move || push_ui(&a_import, UiAction::MakerImportCode(import_code.clone())),
    ))
    .child(spacer(8.0))
    .child(tail);

    modal_shell(inner)
}

fn level_info_ui(st: &SharedUi, actions: Arc<Mutex<Vec<UiAction>>>) -> View {
    let a_close = actions.clone();
    let a_save = actions.clone();
    let a_focus = actions.clone();

    let field = |label: &'static str, value: String, focus: u8| -> View {
        let a = a_focus.clone();
        let focused = st.info_focus == focus;
        let display = if value.is_empty() {
            "-".to_string()
        } else {
            value.clone()
        };
        FilledTonalButton(
            Modifier::new()
                .fill_max_width()
                .padding(10.0)
                .background(if focused {
                    col(70, 90, 120)
                } else {
                    col(45, 45, 60)
                })
                .clip_rounded(8.0),
            move || push(&a, UiAction::LevelInfoFocus(focus)),
            ButtonConfig::default(),
            move || {
                Column(Modifier::new().align_items(AlignItems::FLEX_START)).child((
                    RText(label).size(11.0).color(col(150, 150, 170)),
                    RText(display.clone()).size(14.0).color(RColor::WHITE),
                ))
            },
        )
    };

    let mut tag_views: Vec<View> = Vec::new();
    for tag in LevelTag::ALL {
        let a = actions.clone();
        let included = st.info_tags.contains(&tag);
        let label = RText(tag.label()).size(12.0).color(if included {
            RColor::WHITE
        } else {
            col(150, 150, 170)
        });
        tag_views.push(mk_chip(label, included, tag_color(tag), move || {
            push(&a, UiAction::LevelInfoToggleTag(tag))
        }));
    }
    let tag_row = Row(Modifier::new().gap(6.0)).child(tag_views);

    // Level Settings: boundary preset, water plane.
    let mut preset_views: Vec<View> = Vec::new();
    for p in crate::maker::level::BoundaryPreset::ALL {
        let a = actions.clone();
        let selected = st.info_preset == Some(p);
        let label = RText(p.label()).size(11.0).color(RColor::WHITE);
        preset_views.push(mk_chip(label, selected, col(90, 90, 120), move || {
            push(&a, UiAction::LevelInfoPreset(p))
        }));
    }
    let preset_row = Row(Modifier::new().gap(6.0)).child(preset_views);

    // Water plane.
    let a_wup = actions.clone();
    let a_wdn = actions.clone();
    let water_label = RText(match st.info_water {
        Some(level) => "  Water y = ".to_string() + &level.to_string() + "  ",
        None => "  No water  ".to_string(),
    });
    let water_minus = mk_button_sm("-", move || push(&a_wdn, UiAction::LevelInfoWaterDelta(-1)));
    let water_plus = mk_button_sm("+", move || push(&a_wup, UiAction::LevelInfoWaterDelta(1)));
    let settings_row = Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER))
        .child(water_minus)
        .child(water_label)
        .child(water_plus);

    // Size control (radius half-extents rx × rz wide, ry tall).
    let a_sup = actions.clone();
    let a_sdn = actions.clone();
    let size_auto = st.info_size_auto;
    let size_label = RText(if size_auto {
        "  Size: auto  ".to_string()
    } else {
        format!(
            "  Size {}×{}×{}  ",
            st.info_size[0], st.info_size[1], st.info_size[2]
        )
    });
    let size_minus = mk_button_sm("-", move || push(&a_sdn, UiAction::LevelInfoSizeDelta(-1)));
    let size_plus = mk_button_sm("+", move || push(&a_sup, UiAction::LevelInfoSizeDelta(1)));
    let a_sauto = actions.clone();
    let size_auto_btn = mk_button_sm("Auto", move || {
        push_ui(&a_sauto, UiAction::LevelInfoSizeAuto)
    });
    let size_row = Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER))
        .child(size_minus)
        .child(size_label)
        .child(size_plus)
        .child(size_auto_btn);

    // Boundary / room height (independent of the x/z size).
    let a_hup = actions.clone();
    let a_hdn = actions.clone();
    let a_hauto = actions.clone();
    let height_label = RText(if st.info_height == 0 {
        "  Height: auto  ".to_string()
    } else {
        format!("  Height: {} cells  ", st.info_height)
    });
    let height_minus = mk_button_sm("-", move || {
        push(&a_hdn, UiAction::LevelInfoHeightDelta(-1))
    });
    let height_plus = mk_button_sm("+", move || push(&a_hup, UiAction::LevelInfoHeightDelta(1)));
    let height_auto_btn = mk_button_sm("Auto", move || {
        push_ui(&a_hauto, UiAction::LevelInfoHeightAuto)
    });
    let height_row = Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER))
        .child(height_minus)
        .child(height_label)
        .child(height_plus)
        .child(height_auto_btn);

    let stats = format!(
        "Blocks: {}   ·   Entities: {}",
        st.info_blocks, st.info_entities
    );

    let inner = Column(
        Modifier::new()
            .width(480.0)
            .padding(24.0)
            .background(col(20, 20, 28))
            .clip_rounded(12.0)
            .align_items(AlignItems::CENTER),
    )
    .child(RText("Level Info").size(32.0).color(RColor::WHITE))
    .child(spacer(6.0))
    .child(
        RText("Click a field to edit · tags show up in Browse")
            .size(12.0)
            .color(col(150, 150, 170)),
    )
    .child(spacer(16.0))
    .child(field("Name", st.info_name.clone(), 0))
    .child(spacer(8.0))
    .child(field("Author", st.info_author.clone(), 1))
    .child(spacer(8.0))
    .child(field("Description", st.info_description.clone(), 2))
    .child(spacer(16.0))
    .child(RText("Tags").size(14.0).color(col(180, 180, 190)))
    .child(spacer(6.0))
    .child(tag_row)
    .child(spacer(16.0))
    .child(RText("Level Settings").size(14.0).color(col(180, 180, 190)))
    .child(spacer(6.0))
    .child(
        RText("Boundary presets: floor caught, walls rim, ceiling cap.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(preset_row)
    .child(spacer(8.0))
    .child(settings_row)
    .child(spacer(12.0))
    .child(
        RText("Type size: shrink from auto box, grow to enlarge.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(size_row)
    .child(spacer(8.0))
    .child(
        RText("Height is the room/wall top; the ceiling sits there.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(6.0))
    .child(height_row)
    .child(spacer(12.0))
    .child(RText(stats).size(13.0).color(col(200, 200, 210)))
    .child(spacer(12.0))
    .child(
        RText("Saving metadata does not reset verification.")
            .size(11.0)
            .color(col(130, 130, 150)),
    )
    .child(spacer(16.0))
    .child(mk_primary_button(
        icon_label(Symbols::SAVE, "Save".into()),
        col(60, 140, 90),
        move || push(&a_save, UiAction::LevelInfoSave),
    ))
    .child(mk_button("Back", col(70, 70, 90), move || {
        push(&a_close, UiAction::LevelInfoClose)
    }));

    modal_shell(inner)
}

fn mk_button(label: &str, _bg: RColor, on_click: impl Fn() + 'static) -> View {
    FilledTonalButton(
        Modifier::new().width(260.0).margin(8.0),
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
