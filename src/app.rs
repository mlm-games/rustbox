use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use repose_bevy::{ReposePlugin, ReposePluginSettings};
use repose_core::{prelude::Modifier, remember};
use repose_ui::overlay::OverlayHandle;

use crate::dev_tools::DevToolsPlugin;
use crate::maker::MakerPlugin;
use crate::menus::{self, UiAction, UiBridge};
use crate::save::SaveData;
use crate::screens::ScreensPlugin;
use crate::theme::ThemePlugin;
use game_utils_bevy::{
    EcosystemPlugin,
    audio::AudioChannels,
    i18n::{self, I18nPlugin, LocaleResources},
    post_process::{ScreenEffectSettings, sync_post_process_settings},
    save::{SaveManager, SavePlugin},
    screen_effects::{CameraBase, FlashWhite},
    transitions::Transition,
};

const TRANSLATION_KEYS: &[&str] = &[
    "app-title",
    "start-game",
    "settings",
    "credits",
    "quit",
    "paused",
    "resume",
    "quit-to-title",
    "save",
    "back",
    "master-volume",
    "sfx-volume",
    "music-volume",
    "language",
    "score",
    "best",
    "controls-hint",
    "loading",
    // Maker toolbar
    "toolbar-play",
    "toolbar-edit",
    "toolbar-blocks",
    "toolbar-entities",
    "toolbar-tracks",
    "toolbar-track-brush",
    "maker-track-hint-idle",
    "maker-track-hint-track",
    "maker-hint-track",
    "toolbar-grass",
    "toolbar-stone",
    "toolbar-hazard",
    "toolbar-goal",
    "toolbar-spawn",
    "toolbar-rotate",
    "toolbar-level-untitled",
    "toolbar-block-brush",
    "toolbar-entity-brush",
    "maker-undo",
    "maker-redo",
    "maker-save",
    "maker-load",
    "maker-new",
    "maker-mode-edit",
    "maker-mode-play",
    "maker-blocks-count",
    "maker-glimmers-count",
    "maker-time",
    "maker-deaths",
    "maker-current",
    "maker-btn-edit",
    "maker-retry",
    "maker-hint-block",
    "maker-hint-play",
    "maker-hint-entity",
    "maker-clear-title",
    "maker-load-title",
    "maker-load-empty",
    "maker-ent-glimmer",
    "maker-ent-pad",
    "maker-ent-seal",
    "maker-ent-drift",
    "toolbar-prowler",
    "toolbar-verified",
    "toolbar-unverified",
    "maker-clear-verified",
    "toolbar-publish",
    "share-title",
    "share-verified",
    "share-unverified",
    "share-export-title",
    "share-export-empty",
    "share-export",
    "share-copy",
    "share-import-title",
    "share-import-hint",
    "share-import",
];

const LOCALES: &[(&str, &str)] = &[
    ("en", include_str!("../assets/locales/en/main.ftl")),
    ("es", include_str!("../assets/locales/es/main.ftl")),
    ("fr", include_str!("../assets/locales/fr/main.ftl")),
    ("de", include_str!("../assets/locales/de/main.ftl")),
    ("ja", include_str!("../assets/locales/ja/main.ftl")),
    ("zh", include_str!("../assets/locales/zh/main.ftl")),
    ("pt", include_str!("../assets/locales/pt/main.ftl")),
];

#[derive(States, Default, Clone, Eq, PartialEq, Debug, Hash)]
pub enum AppState {
    #[default]
    Splash,
    Loading,
    Title,
    InGame,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub struct Paused(pub bool);

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayMenu {
    #[default]
    None,
    Settings,
    Credits,
    Pause,
    LevelClear,
    LoadLevel,
    Share,
}

#[derive(Resource, Default)]
pub struct PendingUnpause(pub Option<Timer>);

#[derive(Resource, Clone)]
pub struct SharedUi {
    pub phase: AppState,
    pub paused: bool,
    pub overlay: OverlayMenu,
    pub master_vol: f32,
    pub sfx_vol: f32,
    pub music_vol: f32,
    pub high_score: u32,
    pub score: u32,
    pub transition_alpha: f32,
    pub flash_alpha: f32,
    pub language: String,
    pub saved_language: String,
    pub available_languages: Vec<String>,
    pub translations: HashMap<String, String>,
    // Maker toolbar state
    pub blocks_placed: u32,
    pub maker_mode_edit: bool,
    pub selected_block: u8,
    pub can_undo: bool,
    pub can_redo: bool,
    pub maker_status: String,
    pub pointer_over_ui: bool,
    // Level clear
    pub clear_time_secs: f32,
    pub clear_deaths: u32,
    // Named slots
    pub level_slots: Vec<String>,
    pub level_name: String,
    // Entity brush + glimmer progress
    pub brush_entities: bool,
    pub selected_entity: u8,
    pub brush_tab: u8,
    pub active_track_label: String,
    pub glimmers_collected: u32,
    pub glimmers_total: u32,
    // Live play stats
    pub play_time_secs: f32,
    pub deaths: u32,
    pub level_verified: bool,
    // Share / publish
    pub export_code: String,
    pub import_code: String,
    pub export_error: Option<String>,
}

impl Default for SharedUi {
    fn default() -> Self {
        Self {
            phase: AppState::Splash,
            paused: false,
            overlay: OverlayMenu::None,
            master_vol: 1.0,
            sfx_vol: 1.0,
            music_vol: 0.8,
            high_score: 0,
            score: 0,
            transition_alpha: 0.0,
            flash_alpha: 0.0,
            language: "en".to_string(),
            saved_language: "en".to_string(),
            available_languages: vec!["en".to_string()],
            translations: HashMap::new(),
            blocks_placed: 0,
            maker_mode_edit: true,
            selected_block: 0,
            can_undo: false,
            can_redo: false,
            maker_status: String::new(),
            pointer_over_ui: false,
            clear_time_secs: 0.0,
            clear_deaths: 0,
            level_slots: vec![],
            level_name: "Untitled Level".to_string(),
            brush_entities: false,
            selected_entity: 0,
            brush_tab: 0,
            active_track_label: String::new(),
            glimmers_collected: 0,
            glimmers_total: 0,
            play_time_secs: 0.0,
            deaths: 0,
            level_verified: false,
            export_code: String::new(),
            import_code: String::new(),
            export_error: None,
        }
    }
}

pub struct AppPlugin;

impl Plugin for AppPlugin {
    fn build(&self, app: &mut App) {
        let shared = Arc::new(Mutex::new(SharedUi::default()));
        let actions = Arc::new(Mutex::new(Vec::<UiAction>::new()));
        let shared_ui = shared.clone();
        let actions_ui = actions.clone();

        app.init_state::<AppState>()
            .insert_resource(Paused(false))
            .insert_resource(OverlayMenu::None)
            .insert_resource(PendingUnpause(None))
            .insert_resource(UiBridge {
                shared: shared.clone(),
                actions: actions.clone(),
            })
            .add_plugins(ReposePlugin::with_settings(
                ReposePluginSettings {
                    clear_alpha: 0.0,
                    compose_every_frame: true,
                    msaa_samples: 1,
                    overlay: true,
                },
                move |_s, _c| {
                    let st = shared_ui.lock().unwrap().clone();
                    let acts = actions_ui.clone();
                    let overlay_rc = remember(OverlayHandle::new);
                    let overlay = (*overlay_rc).clone();
                    let root = menus::compose_root(overlay.clone(), st, acts);
                    overlay.host(Modifier::new().fill_max_size(), root)
                },
            ))
            .add_plugins((
                ThemePlugin,
                EcosystemPlugin::<AppState>::new(I18nPlugin::new(TRANSLATION_KEYS, LOCALES)),
                SavePlugin::<SaveData>::new(SaveManager::new(
                    "com",
                    "mlm-games",
                    "my-ecosystem-bevy",
                    "save.ron",
                    1,
                )),
                ScreensPlugin,
                MakerPlugin,
                DevToolsPlugin,
            ))
            .add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                (
                    sync_shared_ui,
                    sync_post_process_settings::<AppState>,
                    process_ui_actions,
                    handle_pause_input,
                    tick_pending_unpause,
                    sync_virtual_time_with_pause,
                )
                    .chain(),
            );
    }
}

fn setup_camera(mut commands: Commands) {
    // 2D camera for UI overlay (Repose renders into Bevy UI image)
    // Order 1 so it renders after the 3D world camera (order 0) without clearing it.
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 1000.0),
        CameraBase {
            translation: Vec3::new(0.0, 0.0, 1000.0),
            rotation: 0.0,
        },
        ScreenEffectSettings::default(),
    ));
}

fn sync_shared_ui(
    state: Res<State<AppState>>,
    paused: Res<Paused>,
    overlay: Res<OverlayMenu>,
    bridge: Res<UiBridge>,
    save: Res<SaveData>,
    transition: Res<Transition<AppState>>,
    flash: Res<FlashWhite>,
    locale: Res<LocaleResources>,
    mut channels: ResMut<AudioChannels>,
    maker_ui: Option<ResMut<crate::maker::ui_bridge::MakerUi>>,
    level: Option<Res<crate::maker::level::LevelDocument>>,
) {
    let Ok(mut ui) = bridge.shared.lock() else {
        return;
    };
    // Pointer ownership is per-click: reset each frame, then SetPointerOverUi
    // actions (pushed by UI clicks) re-assert it when processed below.
    ui.pointer_over_ui = false;
    if let Some(mut m) = maker_ui {
        m.pointer_over_ui = false;
        ui.phase = state.get().clone();
        ui.paused = paused.0;
        ui.overlay = *overlay;
        ui.high_score = save.high_score;
        ui.blocks_placed = m.blocks_placed;
        ui.score = m.blocks_placed;
        ui.maker_mode_edit = m.mode == crate::maker::mode::MakerMode::Edit;
        ui.selected_block = m.selected as u8;
        ui.can_undo = m.can_undo;
        ui.can_redo = m.can_redo;
        ui.maker_status = m.status.clone();
        ui.clear_time_secs = m.clear_time_secs;
        ui.clear_deaths = m.clear_deaths;
        ui.level_slots = m.level_slots.clone();
        ui.brush_entities = m.brush_entities;
        ui.selected_entity = m.selected_entity;
        ui.brush_tab = m.brush_tab;
        ui.active_track_label = m.active_track_label.clone();
        ui.glimmers_collected = m.glimmers_collected;
        ui.glimmers_total = m.glimmers_total;
        ui.level_verified = m.level_verified;
        ui.export_code = m.export_code.clone();
        ui.import_code = m.import_code.clone();
        ui.export_error = m.export_error.clone();
        ui.play_time_secs = m.play_timer;
        ui.deaths = m.deaths;
    } else {
        ui.phase = state.get().clone();
        ui.paused = paused.0;
        ui.overlay = *overlay;
        ui.high_score = save.high_score;
    }
    if let Some(l) = level {
        ui.level_name = l.data.name.clone();
    }
    if *overlay != OverlayMenu::Settings {
        ui.master_vol = save.settings.master_volume;
        ui.sfx_vol = save.settings.sfx_volume;
        ui.music_vol = save.settings.music_volume;
    }
    ui.transition_alpha = transition.overlay_alpha;
    ui.flash_alpha = flash.amount;
    ui.language = locale.current.clone();
    ui.available_languages = locale.available.clone();
    ui.translations = i18n::get_current_translations(&locale);
    channels.master = save.settings.master_volume;
    channels.sfx = save.settings.sfx_volume;
    channels.music = save.settings.music_volume;
}

fn tick_pending_unpause(
    real: Res<Time<Real>>,
    mut pending: ResMut<PendingUnpause>,
    mut paused: ResMut<Paused>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    let Some(timer) = pending.0.as_mut() else {
        return;
    };
    if timer.tick(real.delta()).just_finished() {
        pending.0 = None;
        paused.0 = false;
        virtual_time.unpause();
    }
}

fn set_vol(bridge: &UiBridge, field: impl Fn(&mut SharedUi) -> &mut f32, v: f32) {
    if let Ok(mut ui) = bridge.shared.lock() {
        *field(&mut ui) = v.clamp(0.0, 1.0);
    }
}

fn process_ui_actions(
    bridge: Res<UiBridge>,
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut save: ResMut<SaveData>,
    mut exit: MessageWriter<AppExit>,
    mut transition: ResMut<Transition<AppState>>,
    manager: Res<SaveManager>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut pending_unpause: ResMut<PendingUnpause>,
    mut locale: ResMut<LocaleResources>,
    mut maker_ui: Option<ResMut<crate::maker::ui_bridge::MakerUi>>,
) {
    use crate::maker::block::BlockKind;
    use crate::maker::entity_data::EntityKind;
    use crate::maker::mode::{BrushTab, MakerMode};
    use crate::maker::ui_bridge::UiCommand;

    let Ok(mut q) = bridge.actions.lock() else {
        return;
    };
    for action in q.drain(..) {
        match action {
            UiAction::StartGame => {
                transition.begin_to_state(AppState::InGame);
            }
            UiAction::OpenSettings => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.saved_language = locale.current.clone();
                }
                *overlay = OverlayMenu::Settings;
            }
            UiAction::OpenCredits => *overlay = OverlayMenu::Credits,
            UiAction::CloseOverlay => {
                if *overlay == OverlayMenu::Settings
                    && let Ok(ui) = bridge.shared.lock()
                {
                    locale.set_locale(&ui.saved_language);
                }
                match *overlay {
                    OverlayMenu::Settings | OverlayMenu::Credits if paused.0 => {
                        *overlay = OverlayMenu::Pause;
                    }
                    OverlayMenu::Pause if paused.0 => {
                        *overlay = OverlayMenu::None;
                        pending_unpause.0 = Some(Timer::from_seconds(0.2, TimerMode::Once));
                    }
                    OverlayMenu::LevelClear => {
                        *overlay = OverlayMenu::None;
                        paused.0 = false;
                        virtual_time.unpause();
                        if let Some(ref mut m) = maker_ui {
                            m.commands.push(UiCommand::SetMode(MakerMode::Edit));
                        }
                    }
                    OverlayMenu::LoadLevel => {
                        *overlay = OverlayMenu::None;
                    }
                    _ => {
                        *overlay = OverlayMenu::None;
                    }
                }
            }
            UiAction::Resume => {
                *overlay = OverlayMenu::None;
                pending_unpause.0 = Some(Timer::from_seconds(0.2, TimerMode::Once));
            }
            UiAction::QuitToTitle => {
                paused.0 = false;
                *overlay = OverlayMenu::None;
                pending_unpause.0 = None;
                virtual_time.unpause();
                transition.begin_to_state(AppState::Title);
            }
            UiAction::QuitApp => {
                exit.write(AppExit::Success);
            }
            UiAction::SetMasterVol(v) => set_vol(&bridge, |ui| &mut ui.master_vol, v),
            UiAction::SetSfxVol(v) => set_vol(&bridge, |ui| &mut ui.sfx_vol, v),
            UiAction::SetMusicVol(v) => set_vol(&bridge, |ui| &mut ui.music_vol, v),
            UiAction::SaveSettings => {
                if let Ok(ui) = bridge.shared.lock() {
                    save.settings.master_volume = ui.master_vol;
                    save.settings.sfx_volume = ui.sfx_vol;
                    save.settings.music_volume = ui.music_vol;
                    save.settings.language = locale.current.clone();
                }
                let _ = manager.save(&*save);
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.saved_language = locale.current.clone();
                }
                if paused.0 {
                    *overlay = OverlayMenu::Pause;
                } else {
                    *overlay = OverlayMenu::None;
                }
            }
            UiAction::NextLanguage => {
                let available = locale.available.clone();
                let current = locale.current.clone();
                let idx = available.iter().position(|l| *l == current).unwrap_or(0);
                let next = (idx + 1) % available.len();
                if let Some(next_locale) = available.get(next) {
                    locale.set_locale(next_locale);
                }
            }
            UiAction::SetLanguage(ref lang) => {
                if locale.available.contains(lang) {
                    locale.set_locale(lang);
                }
            }
            // Maker actions
            UiAction::MakerToggleMode => {
                if let Some(ref mut m) = maker_ui {
                    let next = if m.mode == MakerMode::Edit {
                        MakerMode::Play
                    } else {
                        MakerMode::Edit
                    };
                    m.commands.push(UiCommand::SetMode(next));
                }
            }
            UiAction::MakerSelectBlock(i) => {
                if let Some(ref mut m) = maker_ui {
                    let kind = match i {
                        1 => BlockKind::Stone,
                        2 => BlockKind::Hazard,
                        3 => BlockKind::Goal,
                        4 => BlockKind::Spawn,
                        _ => BlockKind::Grass,
                    };
                    m.commands.push(UiCommand::SelectBlock(kind));
                }
            }
            UiAction::MakerToggleBrushTab => {
                if let Some(ref mut m) = maker_ui {
                    let next = (m.brush_tab + 1) % 3;
                    let tab = match next {
                        1 => BrushTab::Entities,
                        2 => BrushTab::Tracks,
                        _ => BrushTab::Blocks,
                    };
                    m.commands.push(UiCommand::SetBrushTab(tab));
                }
            }
            UiAction::MakerSelectEntity(i) => {
                if let Some(ref mut m) = maker_ui {
                    let kind = match i {
                        1 => EntityKind::LaunchPad,
                        2 => EntityKind::Seal,
                        3 => EntityKind::DriftPlate,
                        4 => EntityKind::Prowler,
                        _ => EntityKind::Glimmer,
                    };
                    m.commands.push(UiCommand::SelectEntity(kind));
                }
            }
            UiAction::MakerRotateBrush => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::Rotate);
                }
            }
            UiAction::MakerUndo => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::Undo);
                }
            }
            UiAction::MakerRedo => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::Redo);
                }
            }
            UiAction::MakerSave => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::Save);
                }
            }
            UiAction::MakerPublish => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::Publish);
                }
            }
            UiAction::MakerExportCode => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::ExportCode);
                }
            }
            UiAction::MakerImportCode(ref code) => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::ImportCode(code.clone()));
                }
            }
            UiAction::MakerCopyCode => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::CopyCode);
                }
            }
            UiAction::MakerLoad => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::Load);
                }
            }
            UiAction::MakerNewLevel => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::NewLevel);
                }
            }
            UiAction::MakerOpenLoadPanel => {
                *overlay = OverlayMenu::LoadLevel;
            }
            UiAction::MakerLoadSlot(ref name) => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::LoadSlot(name.clone()));
                }
                *overlay = OverlayMenu::None;
            }
            UiAction::MakerSaveAs(ref name) => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::SaveAs(name.clone()));
                }
            }
            UiAction::MakerDismissClear => {
                *overlay = OverlayMenu::None;
                paused.0 = false;
                virtual_time.unpause();
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::SetMode(MakerMode::Edit));
                }
            }
            UiAction::MakerRetry => {
                *overlay = OverlayMenu::None;
                paused.0 = false;
                virtual_time.unpause();
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::RetryPlay);
                }
            }
            UiAction::SetPointerOverUi(v) => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.pointer_over_ui = v;
                }
                if let Some(ref mut m) = maker_ui {
                    m.pointer_over_ui = v;
                }
            }
        }
    }
}

fn handle_pause_input(
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    mut paused: ResMut<Paused>,
    mut overlay: ResMut<OverlayMenu>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut pending_unpause: ResMut<PendingUnpause>,
    transition: Res<Transition<AppState>>,
) {
    if *state.get() != AppState::InGame {
        return;
    }
    if transition.block_input {
        return;
    }
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    match *overlay {
        OverlayMenu::None if !paused.0 => {
            paused.0 = true;
            *overlay = OverlayMenu::Pause;
            virtual_time.pause();
            pending_unpause.0 = None;
        }
        OverlayMenu::Pause => {
            *overlay = OverlayMenu::None;
            pending_unpause.0 = Some(Timer::from_seconds(0.2, TimerMode::Once));
        }
        OverlayMenu::LevelClear | OverlayMenu::LoadLevel => {
            *overlay = OverlayMenu::None;
            paused.0 = false;
            virtual_time.unpause();
        }
        OverlayMenu::Settings | OverlayMenu::Credits => {
            if paused.0 {
                *overlay = OverlayMenu::Pause;
            } else {
                *overlay = OverlayMenu::None;
            }
        }
        _ => {}
    }
}

fn sync_virtual_time_with_pause(paused: Res<Paused>, mut virtual_time: ResMut<Time<Virtual>>) {
    if paused.0 {
        if !virtual_time.is_paused() {
            virtual_time.pause();
        }
    } else if virtual_time.is_paused() {
        virtual_time.unpause();
    }
}
