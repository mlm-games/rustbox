use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;

use crate::app::OverlayMenu;

use super::block::{ALL_BLOCK_SHAPES, BlockKind};
use super::campaign::{self, LevelSource};
use super::catalog;
use super::commands::{CommandHistory, EditCommand};
use super::entity_data::{EntityData, EntityKind};
use super::level::{BoundaryPreset, LevelDocument};
use super::mode::{
    ActiveLinkChannel, BlockBrush, BrushTab, InputCapture, MakerMode, MakerStats, PlaceYaw,
    SelectedEntity, SelectedEntityKind,
};
use super::online::OnlineRequest;
use super::storage::{self, AUTOSAVE_KEY, LevelStorage, apply_level_data};
use super::track::{ActiveTrack, TrackData, TrackId, TrackMode};
use rustbox_format::api::{LevelMeta, UploadMetadata};
use rustbox_format::file::FORMAT_VERSION;

#[derive(Clone, Debug)]
pub enum UiCommand {
    SelectBlock(BlockKind),
    CycleShape,
    ToggleWaterlog,
    RotateBrush,
    SetMode(MakerMode),
    SetBrushTab(BrushTab),
    SelectEntity(EntityKind),
    Rotate,
    Undo,
    Redo,
    Save,
    Load,
    NewLevel,
    SaveAs(String),
    LoadSlot(String),
    RetryPlay,
    RefreshSlotList,
    Publish,
    ExportCode,
    ImportCode(String),
    CopyCode,
    DeltaEntityParam(u32, f32),
    DeltaEntityYaw(u32, f32),
    DeltaEntityLink(u32, i32),
    CycleLinkChannel,
    CycleEntityTrack(u32),
    DeleteEntity(u32),
    ToggleTrackMode(u32),
    DeltaTrackSpeed(u32, f32),
    ReverseTrack(u32),
    DeleteTrack(u32),
    PlayBundled(usize),
    RemixCurrent,
    PlayCatalogEntry(String),
    EditCatalogEntry(String),
    AddToCollection,
    OpenLevelInfo,
    SaveMetadata,
    /// Apply a named boundary preset (floor / walls / ceiling combo).
    ApplyPreset(BoundaryPreset),
    /// Set the global water plane (None = no water).
    SetWaterLevel(Option<i32>),
    /// Grow/shrink the play-size half-extents by `delta` on every axis
    /// (clamped so it never cuts off existing blocks).
    SizeDelta(i32),
    /// Revert the play-size to auto (derived from content).
    SizeAuto,
    /// Set the boundary wall/room height in cells (0 = auto from size).
    SetBoundaryHeight(i32),
    OnlineUpload,
}

#[derive(Resource, Default)]
pub struct MakerUi {
    pub mode: MakerMode,
    pub selected: BlockKind,
    pub brush_shape: u8,
    pub brush_rot: u8,
    pub waterlogged: bool,
    pub brush_entities: bool,
    pub selected_entity: u8,
    pub brush_tab: u8,
    pub active_track: Option<TrackId>,
    pub active_track_label: String,
    pub blocks_placed: u32,
    pub can_undo: bool,
    pub can_redo: bool,
    pub status: String,
    pub status_timer: f32,

    pub commands: Vec<UiCommand>,
    pub pointer_over_ui: bool,
    pub keyboard_captured: bool,

    pub clear_time_secs: f32,
    pub clear_deaths: u32,
    pub level_slots: Vec<String>,
    pub play_timer: f32,
    pub deaths: u32,
    pub goal_latched: bool,
    pub glimmers_collected: u32,
    pub glimmers_total: u32,
    pub score: u32,
    pub level_verified: bool,
    /// True when this run's clear created the level's first record, so the clear screen can
    /// show a "first clear" instead of a "beat the author" comparison.
    pub first_clear: bool,
    /// True when the current player cleared and set/beat the world record.
    pub new_record: bool,
    /// True when the current player is the level's author (maker). Author
    /// clear times never count toward the world record.
    pub player_is_author: bool,
    pub export_code: String,
    pub import_code: String,
    pub export_error: Option<String>,
    pub selected_entity_data: Option<EntityData>,
    pub active_track_data: Option<TrackData>,
    pub track_ids: Vec<TrackId>,
    pub mirror: u8,
    pub link_channel: u32,
    pub catalog: Vec<crate::maker::catalog::LevelSummary>,
    pub browse_query: String,
    pub browse_include_tags: Vec<crate::maker::level::LevelTag>,
    pub browse_verified_only: bool,
    pub browse_difficulty: Option<u8>,
    pub browse_sort: u8,
    pub browse_confirm_delete: Option<String>,
    /// Key of the currently selected level card (identity-only grid).
    pub browse_selected: Option<String>,

    /// Key the current level is saved under (slot / collection entry), used by
    /// the Level Info metadata editor so it persists to the browsable copy.
    pub current_key: Option<String>,
    pub info_name: String,
    pub info_author: String,
    pub info_description: String,
    pub info_tags: Vec<crate::maker::level::LevelTag>,
    pub info_focus: u8,
    /// Which boundary preset the current flags match (`None` = custom combo).
    pub info_preset: Option<crate::maker::level::BoundaryPreset>,
    pub info_water: Option<i32>,
    pub info_size: [i32; 3],
    pub info_size_auto: bool,
    /// Boundary wall/room height in cells (0 = auto from level size).
    pub info_height: i32,
    /// Level stats shown in the panel: placed blocks / entities.
    pub info_blocks: usize,
    pub info_entities: usize,

    /// Online level-sharing state.
    pub online_levels: Vec<LevelMeta>,
    pub online_query: String,
    pub online_token: String,
    /// 0 = new, 1 = name, 2 = likes, 3 = plays (client-side secondary ordering).
    pub online_sort: u8,
    /// Online shelf tab: 0 = New, 1 = Popular, 2 = Hot (recency-weighted).
    pub online_shelf: u8,
    /// Id of the currently selected online level card (detail panel).
    pub online_selected: Option<u64>,
    /// Dedicated "Level / Maker ID" search field, distinct from the query box.
    pub online_id_query: String,
    pub online_loading: bool,
    /// Requests the flush system turns into fetches (kept here so
    /// `drain_ui_commands` stays under Bevy's 16-param system limit).
    pub online_pending: Vec<OnlineRequest>,
}

impl MakerUi {
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_timer = 2.5;
    }
}

pub fn push_ui_state(
    time: Res<Time>,
    mode: Res<MakerMode>,
    brush: Res<BlockBrush>,
    tab: Res<BrushTab>,
    sel_e: Res<SelectedEntityKind>,
    stats: Res<MakerStats>,
    history: Res<CommandHistory>,
    active: Res<ActiveTrack>,
    sel_ent: Res<SelectedEntity>,
    mirror: Res<super::mode::MirrorMode>,
    channel: Res<ActiveLinkChannel>,
    level: Res<LevelDocument>,
    mut ui: ResMut<MakerUi>,
) {
    ui.mode = *mode;
    ui.selected = brush.kind;
    ui.brush_shape = brush.shape as u8;
    ui.brush_rot = brush.rot;
    ui.waterlogged = brush.waterlogged;
    ui.brush_entities = *tab == BrushTab::Entities;
    ui.selected_entity = match sel_e.0 {
        EntityKind::Glimmer => 0,
        EntityKind::LaunchPad => 1,
        EntityKind::Seal => 2,
        EntityKind::DriftPlate => 3,
        EntityKind::Prowler => 4,
        EntityKind::TriggerOrb => 5,
        EntityKind::RelayGate => 6,
    };
    ui.brush_tab = match *tab {
        BrushTab::Blocks => 0,
        BrushTab::Entities => 1,
        BrushTab::Tracks => 2,
    };
    ui.active_track = active.0;
    ui.selected_entity_data = sel_ent.0.and_then(|id| level.entity_by_id(id)).cloned();
    ui.active_track_data = active.0.and_then(|id| level.track(id)).cloned();
    ui.track_ids = level.data.tracks.iter().map(|t| t.id).collect();
    ui.mirror = mirror.0;
    ui.link_channel = channel.0;
    ui.active_track_label = active
        .0
        .and_then(|id| level.track(id).map(|t| (id, t)))
        .map(|(id, t)| {
            let mode = match t.mode {
                TrackMode::Loop => "Loop",
                TrackMode::PingPong => "PingPong",
            };
            format!("Track #{id} · {mode} · {:.1} u/s", t.speed)
        })
        .unwrap_or_default();
    ui.blocks_placed = stats.blocks_placed;
    ui.can_undo = !history.undo.is_empty();
    ui.can_redo = !history.redo.is_empty();
    ui.level_verified = level.data.is_verified;
    if ui.status_timer > 0.0 {
        ui.status_timer -= time.delta_secs();
        if ui.status_timer <= 0.0 {
            ui.status.clear();
        }
    }
}

pub fn drain_ui_commands(
    mut ui: ResMut<MakerUi>,
    mut mode: ResMut<MakerMode>,
    mut brush: ResMut<BlockBrush>,
    mut tab: ResMut<BrushTab>,
    mut sel_e: ResMut<SelectedEntityKind>,
    mut place_yaw: ResMut<PlaceYaw>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
    storage: Res<LevelStorage>,
    mut overlay: ResMut<OverlayMenu>,
    mut sel_ent: ResMut<SelectedEntity>,
    mut active: ResMut<ActiveTrack>,
    mut channel: ResMut<ActiveLinkChannel>,
    mut clipboard: ResMut<bevy::clipboard::Clipboard>,
    mut source: ResMut<LevelSource>,
    mut players: Query<(
        &mut Transform,
        &mut super::player::Player,
        &mut super::player::MoveState,
        &mut Visibility,
    )>,
) {
    let commands: Vec<UiCommand> = ui.commands.drain(..).collect();
    for cmd in commands {
        match cmd {
            UiCommand::SelectBlock(kind) => brush.kind = kind,
            UiCommand::CycleShape => {
                let shapes = ALL_BLOCK_SHAPES;
                let idx = shapes.iter().position(|s| *s == brush.shape).unwrap_or(0);
                brush.shape = shapes[(idx + 1) % shapes.len()];
                ui.set_status(format!("Block shape: {}", brush.shape.name()));
            }
            UiCommand::ToggleWaterlog => {
                brush.waterlogged = !brush.waterlogged;
                ui.set_status(if brush.waterlogged {
                    "Waterlogged: on (blocks fill their cell with water)"
                } else {
                    "Waterlogged: off"
                });
            }
            UiCommand::RotateBrush => {
                brush.rot = (brush.rot + 1) % 4;
                ui.set_status(format!("Block rotation: {}°", brush.rot * 90));
            }
            UiCommand::SetMode(m) => *mode = m,
            UiCommand::SetBrushTab(t) => *tab = t,
            UiCommand::SelectEntity(kind) => sel_e.0 = kind,
            UiCommand::Rotate => place_yaw.0 = (place_yaw.0 + 45.0) % 360.0,
            UiCommand::Undo => history.undo(&mut level),
            UiCommand::Redo => history.redo(&mut level),
            UiCommand::Save => match storage::save_level(&storage, &mut level, AUTOSAVE_KEY) {
                Ok(()) => ui.set_status("Saved"),
                Err(e) => ui.set_status(format!("Save failed: {e}")),
            },
            UiCommand::Load => {
                match storage::load_level(&storage, &mut level, &mut history, AUTOSAVE_KEY) {
                    Ok(true) => {
                        sel_ent.0 = None;
                        ui.set_status("Loaded")
                    }
                    Ok(false) => ui.set_status("No save found"),
                    Err(e) => ui.set_status(format!("Load failed: {e}")),
                }
            }
            UiCommand::NewLevel => {
                level.seed_default();
                history.undo.clear();
                history.redo.clear();
                sel_ent.0 = None;
                ui.current_key = None;
                ui.set_status("New level");
            }
            UiCommand::SaveAs(name) => match storage::save_level(&storage, &mut level, &name) {
                Ok(()) => {
                    level.data.name = name.clone();
                    ui.current_key = Some(name.clone());
                    ui.set_status(format!("Saved '{name}'"));
                }
                Err(e) => ui.set_status(format!("Save failed: {e}")),
            },
            UiCommand::LoadSlot(name) => {
                match storage::load_level(&storage, &mut level, &mut history, &name) {
                    Ok(true) => {
                        sel_ent.0 = None;
                        ui.current_key = Some(name.clone());
                        ui.set_status(format!("Loaded '{name}'"))
                    }
                    Ok(false) => ui.set_status("Slot empty"),
                    Err(e) => ui.set_status(format!("Load failed: {e}")),
                }
            }
            UiCommand::RetryPlay => {
                ui.goal_latched = false;
                ui.play_timer = 0.0;
                ui.deaths = 0;
                *mode = MakerMode::Play;
                for (mut tf, mut player, mut move_state, mut vis) in &mut players {
                    super::player::reset_player(
                        &mut tf,
                        &mut player,
                        &mut move_state,
                        &mut vis,
                        &level,
                    );
                }
            }
            UiCommand::RefreshSlotList => {
                ui.level_slots = storage::list_slots(&storage);
            }
            UiCommand::Publish => {
                if level.data.is_verified {
                    match storage::export_level_code(&level.data) {
                        Ok(code) => {
                            ui.export_code = code;
                            ui.export_error = None;
                            *overlay = OverlayMenu::Share;
                        }
                        Err(e) => {
                            ui.export_error = Some(format!("Export failed: {e}"));
                            ui.export_code.clear();
                            *overlay = OverlayMenu::Share;
                        }
                    }
                } else {
                    ui.set_status("Beat the level before publishing.");
                }
            }
            UiCommand::ExportCode => match storage::export_level_code(&level.data) {
                Ok(code) => {
                    ui.export_code = code;
                    ui.export_error = None;
                }
                Err(e) => ui.export_error = Some(format!("Export failed: {e}")),
            },
            UiCommand::ImportCode(code) => {
                let code = code.trim();
                match storage::import_level_code(code) {
                    Ok(data) => {
                        apply_level_data(&mut level, &mut history, data);
                        ui.import_code.clear();
                        ui.export_code.clear();
                        ui.current_key = None;
                        sel_ent.0 = None;
                        *overlay = OverlayMenu::None;
                        *source = LevelSource::Imported;
                        *mode = MakerMode::Play;
                        ui.set_status("Level imported!");
                    }
                    Err(e) => ui.set_status(format!("Bad code: {e}")),
                }
            }
            UiCommand::PlayBundled(i) => {
                if let Some(data) = campaign::load_bundled(i) {
                    level.replace_data(data);
                    history.undo.clear();
                    history.redo.clear();
                    sel_ent.0 = None;
                    *source = LevelSource::Bundled(i);
                    *mode = MakerMode::Play; // straight into gameplay
                    ui.set_status(format!("Playing: {}", campaign::BUNDLED_LEVELS[i].name));
                }
            }
            UiCommand::RemixCurrent => {
                level.data.name = format!("Remix of {}", level.data.name);
                crate::maker::commands::invalidate_verification(&mut level); // must re-beat
                *source = LevelSource::Editor;
                *mode = MakerMode::Edit;
                sel_ent.0 = None;
                ui.current_key = None;
                ui.set_status("Remixing... level is yours now. Beat it to share!");
            }
            UiCommand::PlayCatalogEntry(key) => {
                match storage::load_level(&storage, &mut level, &mut history, &key) {
                    Ok(true) => {
                        ui.play_timer = 0.0;
                        ui.deaths = 0;
                        ui.goal_latched = false;
                        ui.clear_time_secs = 0.0;
                        ui.clear_deaths = 0;
                        ui.current_key = Some(key);
                        sel_ent.0 = None;
                        *source = LevelSource::Imported;
                        *mode = MakerMode::Play;
                        ui.set_status(format!("Playing: {}", level.data.name));
                    }
                    Ok(false) => ui.set_status("Level not found."),
                    Err(e) => ui.set_status(format!("Load failed: {e}")),
                }
            }
            UiCommand::EditCatalogEntry(key) => {
                match storage::load_level(&storage, &mut level, &mut history, &key) {
                    Ok(true) => {
                        ui.current_key = Some(key);
                        sel_ent.0 = None;
                        *source = LevelSource::Editor;
                        *mode = MakerMode::Edit;
                        ui.set_status(format!("Editing: {}", level.data.name));
                    }
                    Ok(false) => ui.set_status("Level not found."),
                    Err(e) => ui.set_status(format!("Load failed: {e}")),
                }
            }
            UiCommand::AddToCollection => {
                if let LevelSource::Bundled(_) = *source {
                    ui.set_status("Remix bundled levels before saving.");
                } else {
                    match storage::save_to_collection(&storage, &mut level) {
                        Ok(key) => {
                            ui.current_key = Some(key);
                            ui.set_status("Saved to your collection.");
                            ui.catalog = catalog::build_catalog(&storage);
                        }
                        Err(e) => ui.set_status(format!("Save failed: {e}")),
                    }
                }
            }
            UiCommand::OpenLevelInfo => {
                ui.info_name = level.data.name.clone();
                ui.info_author = level.data.author.clone();
                ui.info_description = level.data.description.clone();
                ui.info_tags = level.data.tags.clone();
                ui.info_focus = 0;
                ui.info_preset = level.data.boundary.boundary_preset();
                ui.info_water = level.data.water_level;
                ui.info_size = level.play_size();
                ui.info_size_auto = level.data.size.is_none();
                ui.info_height = level.data.boundary.height;
                ui.info_blocks = level.map.len();
                ui.info_entities = level.data.entities.len();
                *overlay = OverlayMenu::LevelInfo;
            }
            UiCommand::ApplyPreset(preset) => {
                level.data.boundary = preset.config();
                level.mark_all_dirty();
                ui.info_preset = Some(preset);
                ui.set_status(format!("Boundary: {}", preset.label()));
            }
            UiCommand::SetWaterLevel(level_) => {
                level.data.water_level = level_;
                level.mark_all_dirty();
                ui.set_status(match level_ {
                    Some(y) => format!("Water plane at y={y}"),
                    None => "Water: off".to_string(),
                });
            }
            UiCommand::SizeDelta(delta) => {
                let base = level.data.size.unwrap_or_else(|| level.play_size());
                let mut next = [base[0] + delta, base[1] + delta, base[2] + delta];
                // Never let the box cut off placed blocks (or dip below 1).
                let mut need = [1, 1, 1];
                for b in level.map.values() {
                    need[0] = need[0].max(b.position[0].abs());
                    need[1] = need[1].max(b.position[1]);
                    need[2] = need[2].max(b.position[2].abs());
                }
                for i in 0..3 {
                    next[i] = next[i].max(need[i]);
                }
                level.data.size = Some(next);
                level.mark_all_dirty();
                ui.info_size = next;
                ui.info_size_auto = false;
                ui.set_status(format!("Size {}×{}×{}", next[0], next[1], next[2]));
            }
            UiCommand::SizeAuto => {
                level.data.size = None;
                level.mark_all_dirty();
                ui.info_size = level.play_size();
                ui.info_size_auto = true;
                ui.set_status("Size: auto (from content)");
            }
            UiCommand::SetBoundaryHeight(h) => {
                let h = h.max(0);
                level.data.boundary.height = h;
                level.mark_all_dirty();
                ui.info_height = h;
                ui.set_status(if h == 0 {
                    "Wall height: auto (from size)".to_string()
                } else {
                    format!("Wall height: {h} cells")
                });
            }
            UiCommand::SaveMetadata => {
                level.data.name = ui.info_name.clone();
                level.data.author = ui.info_author.clone();
                level.data.description = ui.info_description.clone();
                level.data.tags = ui.info_tags.clone();
                if level.data.created_at == 0 {
                    level.data.created_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                }
                let key = ui
                    .current_key
                    .clone()
                    .unwrap_or_else(|| AUTOSAVE_KEY.to_string());
                match storage::save_level(&storage, &mut level, &key) {
                    Ok(()) => {
                        ui.catalog = catalog::build_catalog(&storage);
                        ui.set_status("Level info saved.");
                        *overlay = OverlayMenu::None;
                    }
                    Err(e) => ui.set_status(format!("Save failed: {e}")),
                }
            }
            UiCommand::CopyCode => {
                if ui.export_code.is_empty() {
                    ui.set_status("No code to copy yet.");
                } else if clipboard.set_text(ui.export_code.clone()).is_ok() {
                    ui.set_status("Code copied!");
                } else {
                    ui.set_status("Clipboard unavailable.");
                }
            }
            UiCommand::OnlineUpload => {
                if !level.data.is_verified {
                    ui.set_status("Beat the level before publishing.");
                    continue;
                }
                let meta = UploadMetadata {
                    name: level.data.name.clone(),
                    description: level.data.description.clone(),
                    tags: level
                        .data
                        .tags
                        .iter()
                        .map(|t| t.label().to_string())
                        .collect(),
                    format_version: FORMAT_VERSION,
                    game_version: env!("CARGO_PKG_VERSION").to_string(),
                };
                ui.online_pending.push(OnlineRequest::Upload {
                    meta,
                    data: level.data.clone(),
                });
                ui.set_status("Uploading...");
            }
            UiCommand::DeltaEntityParam(id, delta) => {
                if let Some(e) = level.entity_by_id(id) {
                    let old = e.param;
                    let new = (old + delta).clamp(0.0, 100.0);
                    if (new - old).abs() > 1e-4 {
                        history.apply(&mut level, EditCommand::SetEntityParam { id, old, new });
                    }
                }
            }
            UiCommand::DeltaEntityYaw(id, delta) => {
                if let Some(e) = level.entity_by_id(id) {
                    let old = e.yaw_deg;
                    let new = (old + delta).rem_euclid(360.0);
                    if (new - old).abs() > 1e-3 {
                        history.apply(&mut level, EditCommand::SetEntityYaw { id, old, new });
                    }
                }
            }
            UiCommand::DeltaEntityLink(id, delta) => {
                if let Some(e) = level.entity_by_id(id) {
                    let old = e.link;
                    let new = (old as i32 + delta).clamp(1, 9) as u32;
                    if new != old {
                        history.apply(&mut level, EditCommand::SetEntityLink { id, old, new });
                    }
                }
            }
            UiCommand::CycleLinkChannel => {
                channel.0 = channel.0 % 9 + 1;
                ui.set_status(format!("Link channel: {}", channel.0));
            }
            UiCommand::CycleEntityTrack(id) => {
                let Some(e) = level.entity_by_id(id) else {
                    continue;
                };
                let old = e.track;
                let next = match old {
                    Some(_) => None,
                    None => {
                        let mut ids = level.data.tracks.iter().map(|t| t.id).collect::<Vec<_>>();
                        ids.sort();
                        ids.first().copied()
                    }
                };
                if next != old {
                    history.apply(
                        &mut level,
                        EditCommand::SetEntityTrack { id, old, new: next },
                    );
                }
            }
            UiCommand::DeleteEntity(id) => {
                if let Some(entity) = level.entity_by_id(id).cloned() {
                    history.apply(&mut level, EditCommand::RemoveEntity { entity });
                    if sel_ent.0 == Some(id) {
                        sel_ent.0 = None;
                    }
                }
            }
            UiCommand::ToggleTrackMode(id) => {
                if let Some(t) = level.track(id) {
                    let old = t.mode;
                    let new = match old {
                        TrackMode::PingPong => TrackMode::Loop,
                        TrackMode::Loop => TrackMode::PingPong,
                    };
                    history.apply(
                        &mut level,
                        EditCommand::SetTrackMode {
                            track_id: id,
                            old,
                            new,
                        },
                    );
                }
            }
            UiCommand::DeltaTrackSpeed(id, delta) => {
                if let Some(t) = level.track(id) {
                    let old = t.speed;
                    let new = (old + delta).clamp(0.5, 10.0);
                    if (new - old).abs() > 1e-4 {
                        history.apply(
                            &mut level,
                            EditCommand::SetTrackSpeed {
                                track_id: id,
                                old,
                                new,
                            },
                        );
                    }
                }
            }
            UiCommand::ReverseTrack(id) => {
                if level.track(id).is_some() {
                    history.apply(&mut level, EditCommand::ReverseTrackPoints { track_id: id });
                }
            }
            UiCommand::DeleteTrack(id) => {
                if let Some(track) = level.track(id).cloned() {
                    history.apply(&mut level, EditCommand::DeleteTrack { track });
                    if active.0 == Some(id) {
                        active.0 = None;
                    }
                }
            }
        }
    }
    ui.level_slots = storage::list_slots(&storage);
}

pub fn share_text_input(
    mut keys: MessageReader<KeyboardInput>,
    overlay: Res<OverlayMenu>,
    mut ui: ResMut<MakerUi>,
) {
    if *overlay != OverlayMenu::Share {
        return;
    }
    for ev in keys.read() {
        if ev.state != ButtonState::Pressed || ev.repeat {
            continue;
        }
        if ev.key_code == KeyCode::Backspace {
            ui.import_code.pop();
        } else if ev.key_code == KeyCode::Enter {
            let code = ui.import_code.clone();
            ui.commands.push(UiCommand::ImportCode(code));
        } else if let Some(text) = &ev.text
            && text.chars().all(|c| !c.is_control())
        {
            ui.import_code.push_str(text);
        }
    }
}

pub fn browse_text_input(
    mut keys: MessageReader<KeyboardInput>,
    overlay: Res<OverlayMenu>,
    mut ui: ResMut<MakerUi>,
) {
    if *overlay != OverlayMenu::Browse {
        return;
    }
    for ev in keys.read() {
        if ev.state != ButtonState::Pressed || ev.repeat {
            continue;
        }
        if ev.key_code == KeyCode::Backspace {
            ui.browse_query.pop();
        } else if let Some(text) = &ev.text
            && text.chars().all(|c| !c.is_control())
        {
            ui.browse_query.push_str(text);
        }
    }
}

pub fn level_info_text_input(
    mut keys: MessageReader<KeyboardInput>,
    overlay: Res<OverlayMenu>,
    mut ui: ResMut<MakerUi>,
) {
    if *overlay != OverlayMenu::LevelInfo {
        return;
    }
    for ev in keys.read() {
        if ev.state != ButtonState::Pressed || ev.repeat {
            continue;
        }
        if ev.key_code == KeyCode::Backspace {
            match ui.info_focus {
                0 => {
                    ui.info_name.pop();
                }
                1 => {
                    ui.info_author.pop();
                }
                _ => {
                    ui.info_description.pop();
                }
            }
        } else if let Some(text) = &ev.text
            && text.chars().all(|c| !c.is_control())
        {
            match ui.info_focus {
                0 => ui.info_name.push_str(text),
                1 => ui.info_author.push_str(text),
                _ => ui.info_description.push_str(text),
            }
        }
    }
}

pub fn update_input_capture(
    ui: Res<MakerUi>,
    paused: Res<crate::app::Paused>,
    transition: Res<game_utils_bevy::transitions::Transition<crate::app::AppState>>,
    overlay: Res<crate::app::OverlayMenu>,
    bridge: Res<crate::menus::UiBridge>,
    mut capture: ResMut<InputCapture>,
) {
    let modal_open =
        paused.0 || transition.block_input || !matches!(*overlay, crate::app::OverlayMenu::None);
    let pending_ui_touch = bridge
        .actions
        .lock()
        .map(|q| {
            q.iter()
                .any(|a| matches!(a, crate::menus::UiAction::SetPointerOverUi(true)))
        })
        .unwrap_or(false);
    capture.ui_wants_pointer = ui.pointer_over_ui || pending_ui_touch || modal_open;
    capture.ui_wants_keyboard = ui.keyboard_captured || modal_open;
}
