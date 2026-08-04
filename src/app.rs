use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use repose_bevy::{ReposePlugin, ReposePluginSettings};
use repose_core::{prelude::Modifier, remember};
use repose_ui::overlay::OverlayHandle;

use crate::dev_tools::DevToolsPlugin;
use crate::maker::MakerPlugin;
use crate::maker::entity_data::EntityData;
use crate::maker::track::TrackData;
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
    "play-levels",
    "create",
    "level-select-title",
    "maker-remix",
    "maker-beat-author",
    "completed",
    "uncleared",
    "paused",
    "resume",
    "quit-to-title",
    "back-to-menu",
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
    "inspector-cell",
    "inspector-value",
    "inspector-impulse",
    "inspector-glimmers",
    "inspector-period",
    "inspector-speed",
    "inspector-yaw",
    "inspector-track",
    "inspector-none",
    "inspector-delete",
    "inspector-points",
    "inspector-mode",
    "inspector-mode-pingpong",
    "inspector-mode-loop",
    "inspector-reverse",
    "inspector-hint",
    "inspector-mirror",
    "toolbar-trigger",
    "toolbar-gate",
    "maker-link-channel",
    "maker-channel-triggered",
    "inspector-channel",
    "inspector-cooldown",
    "inspector-duration",
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
    LevelSelect,
    Browse,
    Online,
    LevelInfo,
}

#[derive(Resource, Default)]
pub struct PendingUnpause(pub Option<Timer>);

#[derive(Resource, Clone)]
pub struct SharedUi {
    pub phase: AppState,
    pub paused: bool,
    pub loading_progress: f32,
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
    /// Live level-limit counters for the editor HUD bottom bar.
    pub limit_blocks: u32,
    pub limit_entities: u32,
    pub limit_tracks: u32,
    pub limit_vertices: u32,
    pub limit_warning: bool,
    pub limit_over: bool,
    pub maker_mode_edit: bool,
    pub selected_block: u8,
    pub brush_shape: u8,
    pub brush_rot: u8,
    pub waterlogged: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub maker_status: String,
    pub pointer_over_ui: bool,
    /// Runtime sign-read dialog (Play mode): `sign_dialog_open` gates the
    /// overlay; `sign_dialog_lines` holds the pre-wrapped text.
    pub sign_dialog_open: bool,
    pub sign_dialog_lines: Vec<String>,
    /// Sign text editor (Edit mode): `sign_editor_open` gates the modal;
    /// `sign_editor_id` is the entity being edited; `sign_editor_text` is the
    /// live text field content.
    pub sign_editor_open: bool,
    pub sign_editor_id: u32,
    pub sign_editor_text: String,
    // Level clear
    pub clear_time_secs: f32,
    pub clear_deaths: u32,
    /// True when this run created the level's first record (own/first clear).
    pub first_clear: bool,
    /// True when the current player's clear set/beat the world record.
    pub new_record: bool,
    /// True when the current player is the level's author.
    pub player_is_author: bool,
    /// World record in ms (fastest non-author clear). Distinct from author_time.
    pub record_ms: Option<u32>,
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
    // Inspector
    pub selected_entity_data: Option<EntityData>,
    pub active_track_data: Option<TrackData>,
    pub track_ids: Vec<u32>,
    pub mirror: u8,
    pub link_channel: u32,
    // Campaign / bundled levels
    pub is_bundled: bool,
    pub campaign_levels: Vec<crate::maker::campaign::CampaignLevelUi>,
    // Browse
    pub browse_levels: Vec<crate::maker::catalog::LevelSummary>,
    pub browse_visible: Vec<crate::maker::catalog::LevelSummary>,
    pub browse_query: String,
    pub browse_include_tags: Vec<crate::maker::level::LevelTag>,
    pub browse_verified_only: bool,
    pub browse_difficulty: Option<u8>,
    pub browse_sort: u8,
    pub browse_confirm_delete: Option<String>,
    /// Selected local level key (detail panel).
    pub browse_selected: Option<String>,
    // Level Info metadata editor
    pub info_name: String,
    pub info_author: String,
    pub info_description: String,
    pub info_tags: Vec<crate::maker::level::LevelTag>,
    pub info_focus: u8,
    pub info_clear_condition: crate::maker::level::ClearCondition,
    // Level Settings (mirrors `LevelDocument` boundary / water on open).
    pub info_preset: Option<crate::maker::level::BoundaryPreset>,
    pub info_water: Option<i32>,
    pub info_size: [i32; 3],
    pub info_size_auto: bool,
    /// Boundary wall/room height in cells (0 = auto from level size).
    pub info_height: i32,
    /// Level stats shown in the panel: placed blocks / entities.
    pub info_blocks: usize,
    pub info_entities: usize,
    // Online level sharing
    pub online_levels: Vec<rustbox_format::api::LevelMeta>,
    /// Generated online previews keyed by server id.
    pub online_previews: HashMap<u64, crate::maker::thumbnail::ThumbPreview>,
    /// Online previews currently being fetched/generated.
    pub online_preview_pending: Vec<u64>,
    pub online_query: String,
    pub online_token: String,
    /// Anonymous creator identity (recovery key = the account; device id is a
    /// local-only abuse signal). Mirrors `MakerUi`; bootstrapped by the maker.
    pub creator_recovery_key: String,
    pub creator_device_id: String,
    /// Human-readable weekly upload quota from `/v1/me`.
    pub creator_quota_text: String,
    /// Non-empty triggers a recovery-key import in `flush_online_requests`.
    pub creator_import_code: String,
    pub online_sort: u8,
    pub online_loading: bool,
    /// Selected online level id (detail panel).
    pub online_selected: Option<u64>,
    /// When set, the online detail panel shows a Confirm/Cancel delete prompt.
    pub online_confirm_delete: Option<u64>,
    /// Shelf tab: 0 = New, 1 = Popular, 2 = Hot.
    pub online_shelf: u8,
    /// Dedicated Level ID search string (backend `FetchById` looks up a single
    /// level id, not a maker id).
    pub online_id_query: String,
}

impl Default for SharedUi {
    fn default() -> Self {
        Self {
            phase: AppState::Splash,
            paused: false,
            loading_progress: 0.0,
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
            limit_blocks: 0,
            limit_entities: 0,
            limit_tracks: 0,
            limit_vertices: 0,
            limit_warning: false,
            limit_over: false,
            maker_mode_edit: true,
            selected_block: 0,
            brush_shape: 0,
            brush_rot: 0,
            waterlogged: false,
            can_undo: false,
            can_redo: false,
            maker_status: String::new(),
            pointer_over_ui: false,
            sign_dialog_open: false,
            sign_dialog_lines: Vec::new(),
            sign_editor_open: false,
            sign_editor_id: 0,
            sign_editor_text: String::new(),
            clear_time_secs: 0.0,
            clear_deaths: 0,
            first_clear: false,
            new_record: false,
            player_is_author: false,
            record_ms: None,
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
            selected_entity_data: None,
            active_track_data: None,
            track_ids: Vec::new(),
            mirror: 0,
            link_channel: 1,
            is_bundled: false,
            campaign_levels: Vec::new(),
            browse_levels: Vec::new(),
            browse_visible: Vec::new(),
            browse_query: String::new(),
            browse_include_tags: Vec::new(),
            browse_verified_only: false,
            browse_difficulty: None,
            browse_sort: 0,
            browse_confirm_delete: None,
            browse_selected: None,
            info_name: String::new(),
            info_author: String::new(),
            info_description: String::new(),
            info_tags: Vec::new(),
            info_focus: 0,
            info_clear_condition: crate::maker::level::ClearCondition::ReachGoal,
            info_preset: None,
            info_water: None,
            info_size: [16, 12, 16],
            info_size_auto: true,
            info_height: 0,
            info_blocks: 0,
            info_entities: 0,
            online_levels: Vec::new(),
            online_previews: HashMap::new(),
            online_preview_pending: Vec::new(),
            online_query: String::new(),
            online_token: String::new(),
            creator_recovery_key: String::new(),
            creator_device_id: String::new(),
            creator_quota_text: String::new(),
            creator_import_code: String::new(),
            online_sort: 0,
            online_loading: false,
            online_selected: None,
            online_confirm_delete: None,
            online_shelf: 0,
            online_id_query: String::new(),
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
                    msaa_samples: if cfg!(debug_assertions) { 1 } else { 4 },
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
    source: Option<Res<crate::maker::campaign::LevelSource>>,
    progress: Option<Res<crate::maker::campaign::CampaignProgress>>,
    loading: Option<Res<crate::asset_tracking::AssetsLoading>>,
    asset_server: Res<AssetServer>,
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
        ui.limit_blocks = m.limit_blocks;
        ui.limit_entities = m.limit_entities;
        ui.limit_tracks = m.limit_tracks;
        ui.limit_vertices = m.limit_vertices;
        ui.limit_warning = m.limit_warning;
        ui.limit_over = m.limit_over;
        ui.score = m.blocks_placed;
        ui.maker_mode_edit = m.mode == crate::maker::mode::MakerMode::Edit;
        ui.selected_block = m.selected as u8;
        ui.brush_shape = m.brush_shape;
        ui.brush_rot = m.brush_rot;
        ui.waterlogged = m.waterlogged;
        ui.can_undo = m.can_undo;
        ui.can_redo = m.can_redo;
        ui.maker_status = m.status.clone();
        ui.sign_dialog_open = m.sign_dialog_open;
        ui.sign_dialog_lines = m.sign_dialog_lines.clone();
        ui.clear_time_secs = m.clear_time_secs;
        ui.clear_deaths = m.clear_deaths;
        ui.first_clear = m.first_clear;
        ui.new_record = m.new_record;
        ui.player_is_author = m.player_is_author;
        ui.level_slots = m.level_slots.clone();
        ui.browse_levels = m.catalog.clone();
        ui.browse_visible = crate::maker::catalog::filter_catalog(
            &m.catalog,
            &m.browse_query,
            &m.browse_include_tags,
            m.browse_verified_only,
            m.browse_difficulty,
            m.browse_sort,
        );
        ui.browse_query = m.browse_query.clone();
        ui.browse_include_tags = m.browse_include_tags.clone();
        ui.browse_verified_only = m.browse_verified_only;
        ui.browse_difficulty = m.browse_difficulty;
        ui.browse_sort = m.browse_sort;
        ui.browse_confirm_delete = m.browse_confirm_delete.clone();
        ui.browse_selected = m.browse_selected.clone();
        ui.info_name = m.info_name.clone();
        ui.info_author = m.info_author.clone();
        ui.info_description = m.info_description.clone();
        ui.info_tags = m.info_tags.clone();
        ui.info_focus = m.info_focus;
        ui.info_clear_condition = m.info_clear_condition;
        ui.info_preset = m.info_preset;
        ui.info_water = m.info_water;
        ui.info_size = m.info_size;
        ui.info_size_auto = m.info_size_auto;
        ui.info_height = m.info_height;
        ui.info_blocks = m.info_blocks;
        ui.info_entities = m.info_entities;
        ui.online_levels = sort_online(&m.online_levels, m.online_sort, m.online_shelf);
        ui.online_previews = m.online_previews.clone();
        ui.online_preview_pending = m.online_preview_pending.clone();
        ui.online_query = m.online_query.clone();
        ui.online_token = m.online_token.clone();
        ui.creator_recovery_key = m.creator_recovery_key.clone();
        ui.creator_device_id = m.creator_device_id.clone();
        ui.creator_quota_text = m.creator_quota_text.clone();
        ui.creator_import_code = m.creator_import_code.clone();
        ui.online_sort = m.online_sort;
        ui.online_shelf = m.online_shelf;
        ui.online_selected = m.online_selected;
        ui.online_confirm_delete = m.online_confirm_delete;
        ui.online_id_query = m.online_id_query.clone();
        ui.online_loading = m.online_loading;
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
        ui.selected_entity_data = m.selected_entity_data.clone();
        ui.active_track_data = m.active_track_data.clone();
        ui.track_ids = m.track_ids.clone();
        ui.mirror = m.mirror;
        ui.link_channel = m.link_channel;
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
        ui.record_ms = l.data.record_ms;
    }
    if let Some(s) = source {
        use crate::maker::campaign::LevelSource;
        ui.is_bundled = *s != LevelSource::Editor;
    }
    ui.campaign_levels = crate::maker::campaign::BUNDLED_LEVELS
        .iter()
        .map(|b| {
            let rec = progress
                .as_deref()
                .map(|p| p.record(b.id))
                .unwrap_or_default();
            crate::maker::campaign::CampaignLevelUi {
                title: b.name.to_string(),
                teaches: b.teaches.to_string(),
                completed: rec.completed,
                best_time: rec.best_time,
                best_deaths: rec.best_deaths,
            }
        })
        .collect();
    if *overlay != OverlayMenu::Settings {
        ui.master_vol = save.settings.master_volume;
        ui.sfx_vol = save.settings.sfx_volume;
        ui.music_vol = save.settings.music_volume;
    }
    ui.loading_progress = match loading {
        Some(l) if !l.0.is_empty() => {
            l.0.iter()
                .filter(|h| asset_server.is_loaded_with_dependencies(h.id()))
                .count() as f32
                / l.0.len() as f32
        }
        _ => 1.0,
    };
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

fn sort_online(
    levels: &[rustbox_format::api::LevelMeta],
    mode: u8,
    shelf: u8,
) -> Vec<rustbox_format::api::LevelMeta> {
    crate::maker::online::sort_online(levels, mode, shelf)
}

fn hot_of(m: &rustbox_format::api::LevelMeta) -> f64 {
    crate::maker::online::hot_of(m)
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
    storage: Res<crate::maker::storage::LevelStorage>,
) {
    use crate::maker::block::BlockKind;
    use crate::maker::entity_data::EntityKind;
    use crate::maker::mode::{BrushTab, MakerMode};
    use crate::maker::online::OnlineRequest;
    use crate::maker::ui_bridge::UiCommand;
    use rustbox_format::api::LevelMeta;

    let Ok(mut q) = bridge.actions.lock() else {
        return;
    };
    for action in q.drain(..) {
        match action {
            UiAction::StartGame => {
                transition.begin_to_state(AppState::Loading);
            }
            UiAction::OpenSettings => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.saved_language = locale.current.clone();
                }
                *overlay = OverlayMenu::Settings;
            }
            UiAction::OpenCredits => *overlay = OverlayMenu::Credits,
            UiAction::OpenLevelSelect => *overlay = OverlayMenu::LevelSelect,
            UiAction::BrowseOpen => {
                if let Some(ref mut m) = maker_ui {
                    m.catalog = crate::maker::catalog::build_catalog(&storage);
                    m.browse_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
                *overlay = OverlayMenu::Browse;
            }
            UiAction::BrowsePlay(key) => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_confirm_delete = None;
                    m.commands.push(UiCommand::PlayCatalogEntry(key));
                }
                *overlay = OverlayMenu::None;
                transition.begin_to_state(AppState::Loading);
            }
            UiAction::BrowseEdit(key) => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_confirm_delete = None;
                    m.commands.push(UiCommand::EditCatalogEntry(key));
                }
                *overlay = OverlayMenu::None;
                transition.begin_to_state(AppState::Loading);
            }
            UiAction::BrowseDelete(key) => {
                if let Some(ref mut m) = maker_ui {
                    if m.browse_confirm_delete.as_deref() == Some(key.as_str()) {
                        let result = if key.starts_with(crate::maker::storage::COLLECTION_PREFIX) {
                            crate::maker::storage::delete_collection(&storage, &key)
                        } else {
                            storage.0.delete(&key)
                        };
                        m.browse_confirm_delete = None;
                        match result {
                            Ok(()) => {
                                m.catalog = crate::maker::catalog::build_catalog(&storage);
                                crate::maker::ui_bridge::reconcile_browse_nav(m);
                                m.set_status("Deleted.");
                            }
                            Err(e) => m.set_status(format!("Delete failed: {e}")),
                        }
                    } else {
                        m.browse_confirm_delete = Some(key);
                    }
                }
            }
            UiAction::BrowseConfirmDelete(key) => {
                if let Some(ref mut m) = maker_ui {
                    let result = if key.starts_with(crate::maker::storage::COLLECTION_PREFIX) {
                        crate::maker::storage::delete_collection(&storage, &key)
                    } else {
                        storage.0.delete(&key)
                    };
                    m.browse_confirm_delete = None;
                    match result {
                        Ok(()) => {
                            m.catalog = crate::maker::catalog::build_catalog(&storage);
                            crate::maker::ui_bridge::reconcile_browse_nav(m);
                            m.set_status("Deleted.");
                        }
                        Err(e) => m.set_status(format!("Delete failed: {e}")),
                    }
                }
            }
            UiAction::BrowseCancelDelete => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_confirm_delete = None;
                }
            }
            UiAction::BrowseSelect(key) => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_selected = Some(key);
                    m.browse_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
            }
            UiAction::BrowseClearSelection => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_selected = None;
                    m.browse_confirm_delete = None;
                }
            }
            UiAction::BrowseToggleTag(tag) => {
                if let Some(ref mut m) = maker_ui {
                    if let Some(pos) = m.browse_include_tags.iter().position(|t| *t == tag) {
                        m.browse_include_tags.remove(pos);
                    } else {
                        m.browse_include_tags.push(tag);
                    }
                    m.browse_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
            }
            UiAction::BrowseToggleVerified => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_verified_only = !m.browse_verified_only;
                    m.browse_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
            }
            UiAction::BrowseSetDifficulty(d) => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_difficulty = d;
                    m.browse_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
            }
            UiAction::BrowseCycleSort => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_sort = (m.browse_sort + 1) % 6;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
            }
            UiAction::BrowseSetQuery(q) => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_query = q;
                    m.browse_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
            }
            UiAction::BrowseClearQuery => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_query.clear();
                    m.browse_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_browse_nav(m);
                }
            }
            UiAction::BrowseAddToCollection => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::AddToCollection);
                }
            }
            UiAction::OnlineOpen => {
                if let Some(ref mut m) = maker_ui {
                    m.browse_confirm_delete = None;
                    let query = m.online_query.clone();
                    m.online_loading = true;
                    m.online_pending.push(OnlineRequest::List {
                        query,
                        limit: 50,
                        offset: 0,
                    });
                    m.online_pending.push(OnlineRequest::Me);
                    m.set_status("Loading online levels...");
                }
                *overlay = OverlayMenu::Online;
            }
            UiAction::OnlineRefresh => {
                if let Some(ref mut m) = maker_ui {
                    let query = m.online_query.clone();
                    m.online_loading = true;
                    m.online_pending.push(OnlineRequest::List {
                        query,
                        limit: 50,
                        offset: 0,
                    });
                    m.set_status("Loading online levels...");
                }
            }
            UiAction::OnlineSelect(id) => {
                if let Some(ref mut m) = maker_ui {
                    m.online_selected = Some(id);
                    m.online_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_online_nav(m);
                }
            }
            UiAction::OnlinePreview(id) => {
                if let Some(ref mut m) = maker_ui {
                    // Cap concurrent preview downloads so a 50-level grid doesn't
                    // stampede the network just to draw cards (LRU evicts later).
                    const MAX_PREVIEW_IN_FLIGHT: usize = 4;

                    if m.online_previews.contains_key(&id)
                        || m.online_preview_pending.contains(&id)
                        || m.online_preview_pending.len() >= MAX_PREVIEW_IN_FLIGHT
                    {
                        continue;
                    }

                    let Some(meta) = m.online_levels.iter().find(|x| x.id == id).cloned() else {
                        continue;
                    };

                    m.online_preview_pending.push(id);
                    m.online_pending
                        .push(OnlineRequest::Download { meta, play: false });
                }
            }
            UiAction::OnlineClearSelection => {
                if let Some(ref mut m) = maker_ui {
                    m.online_selected = None;
                    m.online_confirm_delete = None;
                }
            }
            UiAction::OnlineSetShelf(shelf) => {
                if let Some(ref mut m) = maker_ui {
                    m.online_shelf = shelf;
                    m.online_confirm_delete = None;
                    crate::maker::ui_bridge::reconcile_online_nav(m);
                    if shelf == 3 {
                        m.online_loading = true;
                        m.online_pending.push(OnlineRequest::MyLevels);
                        m.set_status("Loading your levels...");
                    }
                }
            }
            UiAction::OnlineSetIdQuery(q) => {
                if let Some(ref mut m) = maker_ui {
                    m.online_id_query = q;
                }
            }
            UiAction::OnlineSearchId => {
                if let Some(ref mut m) = maker_ui {
                    let id: u64 = m.online_id_query.trim().parse().unwrap_or(0);
                    if id == 0 {
                        m.set_status("Enter a numeric level ID.");
                    } else {
                        m.online_loading = true;
                        m.online_pending.push(OnlineRequest::FetchById(id));
                        m.set_status(format!("Searching #{id}"));
                    }
                }
            }
            UiAction::OnlinePlay(id) => {
                if let Some(ref mut m) = maker_ui {
                    let meta = m
                        .online_levels
                        .iter()
                        .find(|x| x.id == id)
                        .cloned()
                        .unwrap_or_else(|| LevelMeta {
                            id,
                            author: String::new(),
                            name: format!("#{id}"),
                            description: String::new(),
                            tags: Vec::new(),
                            format_version: 0,
                            game_version: String::new(),
                            size_bytes: 0,
                            sha256: String::new(),
                            likes: 0,
                            plays: 0,
                            created_at: String::new(),
                            updated_at: String::new(),
                        });
                    m.online_pending
                        .push(OnlineRequest::Download { meta, play: true });
                    m.set_status("Downloading level...");
                }
                *overlay = OverlayMenu::None;
                transition.begin_to_state(AppState::Loading);
            }
            UiAction::OnlineLike(id) => {
                if let Some(ref mut m) = maker_ui {
                    m.online_pending.push(OnlineRequest::Like { id });
                }
            }
            UiAction::OnlineReport(id) => {
                if let Some(ref mut m) = maker_ui {
                    m.online_pending.push(OnlineRequest::Report { id });
                }
            }
            UiAction::OnlineDelete(id) => {
                if let Some(ref mut m) = maker_ui {
                    if m.online_confirm_delete == Some(id) {
                        m.online_confirm_delete = None;
                        m.online_pending.push(OnlineRequest::Delete { id });
                        m.set_status(format!("Deleting #{id}..."));
                    } else {
                        m.online_confirm_delete = Some(id);
                    }
                }
            }
            UiAction::OnlineDeleteCancel => {
                if let Some(ref mut m) = maker_ui {
                    m.online_confirm_delete = None;
                }
            }
            UiAction::OnlineClearQuery => {
                if let Some(ref mut m) = maker_ui {
                    m.online_query.clear();
                    m.online_loading = true;
                    m.online_pending.push(OnlineRequest::List {
                        query: String::new(),
                        limit: 50,
                        offset: 0,
                    });
                    m.set_status("Loading online levels...");
                }
            }
            UiAction::OnlineSetQuery(q) => {
                if let Some(ref mut m) = maker_ui {
                    m.online_query = q;
                }
            }
            UiAction::OnlineSearch => {
                if let Some(ref mut m) = maker_ui {
                    let query = m.online_query.clone();
                    m.online_loading = true;
                    m.online_pending.push(OnlineRequest::List {
                        query,
                        limit: 50,
                        offset: 0,
                    });
                    m.set_status("Searching online levels...");
                }
            }
            UiAction::OnlineCycleSort => {
                if let Some(ref mut m) = maker_ui {
                    m.online_sort = (m.online_sort + 1) % 4;
                    crate::maker::ui_bridge::reconcile_online_nav(m);
                }
            }
            UiAction::OnlineSetToken(token) => {
                if let Some(ref mut m) = maker_ui {
                    let token = token.trim().to_string();
                    m.online_token = token.clone();
                    let msg = if token.is_empty() {
                        "Upload token cleared."
                    } else {
                        "Upload token set."
                    };
                    m.set_status(msg);
                }
            }
            UiAction::CreatorImport(code) => {
                if let Some(ref mut m) = maker_ui {
                    m.creator_import_code = code;
                }
            }
            UiAction::CreatorCopyKey => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::CopyCreatorKey);
                }
            }
            UiAction::OnlineUpload => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::OnlineUpload);
                }
            }
            UiAction::LevelInfoOpen => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::OpenLevelInfo);
                }
                *overlay = OverlayMenu::LevelInfo;
            }
            UiAction::LevelInfoFocus(focus) => {
                if let Some(ref mut m) = maker_ui {
                    m.info_focus = focus;
                }
            }
            UiAction::LevelInfoToggleTag(tag) => {
                if let Some(ref mut m) = maker_ui {
                    if let Some(pos) = m.info_tags.iter().position(|t| *t == tag) {
                        m.info_tags.remove(pos);
                    } else {
                        m.info_tags.push(tag);
                    }
                }
            }
            UiAction::LevelInfoCycleClearCondition => {
                if let Some(ref mut m) = maker_ui {
                    use crate::maker::level::ClearCondition::*;
                    m.info_clear_condition = match m.info_clear_condition {
                        ReachGoal => CollectAllGlimmers,
                        CollectAllGlimmers => DefeatAllProwlers,
                        DefeatAllProwlers => NoDeath,
                        NoDeath => TimeLimitMs(60_000),
                        TimeLimitMs(_) => ReachGoal,
                    };
                }
            }
            UiAction::LevelInfoTimeLimitDelta(delta_secs) => {
                if let Some(ref mut m) = maker_ui {
                    if let crate::maker::level::ClearCondition::TimeLimitMs(ms) =
                        &mut m.info_clear_condition
                    {
                        let next = (*ms as i64 + delta_secs as i64 * 1000).max(5_000);
                        *ms = next as u32;
                    }
                }
            }
            UiAction::LevelInfoSave => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::SaveMetadata);
                }
                *overlay = OverlayMenu::None;
            }
            UiAction::LevelInfoClose => {
                *overlay = OverlayMenu::None;
            }
            UiAction::LevelInfoPreset(preset) => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::ApplyPreset(preset));
                }
            }
            UiAction::LevelInfoWaterDelta(delta) => {
                if let Some(ref mut m) = maker_ui {
                    let next = match m.info_water {
                        Some(level) if delta > 0 => Some(level + delta),
                        // Turning water off requires stepping it below 0.
                        Some(level) if level + delta < 0 => None,
                        Some(level) => Some((level + delta).max(0)),
                        None if delta > 0 => Some(1),
                        None => None,
                    };
                    m.info_water = next;
                    m.commands.push(UiCommand::SetWaterLevel(next));
                }
            }
            UiAction::LevelInfoSizeDelta(delta) => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::SizeDelta(delta));
                }
            }
            UiAction::LevelInfoSizeAuto => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::SizeAuto);
                }
            }
            UiAction::LevelInfoHeightDelta(delta) => {
                if let Some(ref mut m) = maker_ui {
                    let next = (m.info_height + delta).max(0);
                    m.commands.push(UiCommand::SetBoundaryHeight(next));
                }
            }
            UiAction::LevelInfoHeightAuto => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::SetBoundaryHeight(0));
                }
            }
            UiAction::PlayBundledLevel(i) => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::PlayBundled(i as usize));
                }
                *overlay = OverlayMenu::None;
                transition.begin_to_state(AppState::Loading);
            }
            UiAction::MakerRemix => {
                *overlay = OverlayMenu::None;
                paused.0 = false;
                virtual_time.unpause();
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::RemixCurrent);
                }
            }
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
                        5 => BlockKind::Water,
                        6 => BlockKind::Ice,
                        7 => BlockKind::Spikes,
                        8 => BlockKind::Conveyor,
                        9 => BlockKind::Bounce,
                        10 => BlockKind::Climb,
                        11 => BlockKind::ThinConveyor,
                        12 => BlockKind::OnOffConveyorA,
                        13 => BlockKind::OnOffConveyorB,
                        14 => BlockKind::HangRail,
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
                        5 => EntityKind::TriggerOrb,
                        6 => EntityKind::RelayGate,
                        7 => EntityKind::Checkpoint,
                        8 => EntityKind::Teleporter,
                        9 => EntityKind::Fan,
                        10 => EntityKind::Bumper,
                        11 => EntityKind::Crate,
                        12 => EntityKind::Key,
                        13 => EntityKind::LockGate,
                        14 => EntityKind::HealOrb,
                        15 => EntityKind::SpeedRing,
                        16 => EntityKind::CrumblePlate,
                        17 => EntityKind::Cannon,
                        18 => EntityKind::OnOffSwitch,
                        19 => EntityKind::TossCrate,
                        20 => EntityKind::Sign,
                        _ => EntityKind::Glimmer,
                    };
                    m.commands.push(UiCommand::SelectEntity(kind));
                }
            }
            UiAction::MakerCycleLinkChannel => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::CycleLinkChannel);
                }
            }
            UiAction::MakerRotateBrush => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::Rotate);
                }
            }
            UiAction::MakerRotateBrushBlock => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::RotateBrush);
                }
            }
            UiAction::MakerCycleShape => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::CycleShape);
                }
            }
            UiAction::MakerToggleWaterlog => {
                if let Some(ref mut m) = maker_ui {
                    m.commands.push(UiCommand::ToggleWaterlog);
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
            UiAction::MakerInspParamDelta(delta) => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                {
                    m.commands.push(UiCommand::DeltaEntityParam(id, delta));
                }
            }
            UiAction::MakerInspCycleContents => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                {
                    m.commands.push(UiCommand::CycleEntityContents(id));
                }
            }
            UiAction::MakerInspContentsDelta(delta) => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                {
                    m.commands.push(UiCommand::DeltaEntityContents(id, delta));
                }
            }
            UiAction::MakerInspYawDelta(delta) => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                {
                    m.commands.push(UiCommand::DeltaEntityYaw(id, delta));
                }
            }
            UiAction::MakerInspLinkDelta(delta) => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                {
                    m.commands.push(UiCommand::DeltaEntityLink(id, delta));
                }
            }
            UiAction::MakerInspTrackCycle => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                {
                    m.commands.push(UiCommand::CycleEntityTrack(id));
                }
            }
            UiAction::MakerInspDeleteEntity => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                {
                    m.commands.push(UiCommand::DeleteEntity(id));
                }
            }
            UiAction::MakerInspTrackModeToggle => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.active_track_data.as_ref().map(|t| t.id)
                {
                    m.commands.push(UiCommand::ToggleTrackMode(id));
                }
            }
            UiAction::MakerInspTrackSpeedDelta(delta) => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.active_track_data.as_ref().map(|t| t.id)
                {
                    m.commands.push(UiCommand::DeltaTrackSpeed(id, delta));
                }
            }
            UiAction::MakerInspTrackReverse => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.active_track_data.as_ref().map(|t| t.id)
                {
                    m.commands.push(UiCommand::ReverseTrack(id));
                }
            }
            UiAction::MakerInspTrackDelete => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.active_track_data.as_ref().map(|t| t.id)
                {
                    m.commands.push(UiCommand::DeleteTrack(id));
                }
            }
            UiAction::MakerInspEditSignText => {
                if let Some(ref mut m) = maker_ui
                    && let Some(id) = m.selected_entity_data.as_ref().map(|e| e.id)
                    && let Ok(mut ui) = bridge.shared.lock()
                {
                    ui.sign_editor_id = id;
                    ui.sign_editor_text = m
                        .selected_entity_data
                        .as_ref()
                        .map(|e| e.sign_text.clone())
                        .unwrap_or_default();
                    ui.sign_editor_open = true;
                }
            }
            UiAction::MakerInspSetSignText(text) => {
                if let Some(ref mut m) = maker_ui
                    && let Ok(ui) = bridge.shared.lock()
                    && ui.sign_editor_open
                {
                    let id = ui.sign_editor_id;
                    m.commands.push(UiCommand::SetEntitySignText { id, text });
                }
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.sign_editor_open = false;
                    ui.sign_editor_text.clear();
                }
            }
            UiAction::MakerInspCancelSignText => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.sign_editor_open = false;
                    ui.sign_editor_text.clear();
                }
            }
            UiAction::MakerCloseSignDialog => {
                if let Ok(mut ui) = bridge.shared.lock() {
                    ui.sign_dialog_open = false;
                    ui.sign_dialog_lines.clear();
                }
                if let Some(ref mut m) = maker_ui {
                    m.sign_dialog_open = false;
                    m.sign_dialog_lines.clear();
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
            UiAction::SetKeyboardCaptured(v) => {
                if let Some(ref mut m) = maker_ui {
                    m.keyboard_captured = v;
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
        OverlayMenu::LevelClear
        | OverlayMenu::LoadLevel
        | OverlayMenu::LevelInfo
        | OverlayMenu::Online => {
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
