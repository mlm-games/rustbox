use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;

use crate::app::OverlayMenu;

use super::block::BlockKind;
use super::commands::CommandHistory;
use super::entity_data::EntityKind;
use super::level::LevelDocument;
use super::mode::{
    BrushTab, InputCapture, MakerMode, MakerStats, PlaceYaw, SelectedBlockKind, SelectedEntityKind,
};
use super::storage::{self, AUTOSAVE_KEY, LevelStorage, apply_level_data};
use super::track::{ActiveTrack, TrackId, TrackMode};

#[derive(Clone, Debug)]
pub enum UiCommand {
    SelectBlock(BlockKind),
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
}

#[derive(Resource, Default)]
pub struct MakerUi {
    pub mode: MakerMode,
    pub selected: BlockKind,
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
    pub export_code: String,
    pub import_code: String,
    pub export_error: Option<String>,
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
    selected: Res<SelectedBlockKind>,
    tab: Res<BrushTab>,
    sel_e: Res<SelectedEntityKind>,
    stats: Res<MakerStats>,
    history: Res<CommandHistory>,
    active: Res<ActiveTrack>,
    level: Res<LevelDocument>,
    mut ui: ResMut<MakerUi>,
) {
    ui.mode = *mode;
    ui.selected = selected.0;
    ui.brush_entities = *tab == BrushTab::Entities;
    ui.selected_entity = match sel_e.0 {
        EntityKind::Glimmer => 0,
        EntityKind::LaunchPad => 1,
        EntityKind::Seal => 2,
        EntityKind::DriftPlate => 3,
        EntityKind::Prowler => 4,
    };
    ui.brush_tab = match *tab {
        BrushTab::Blocks => 0,
        BrushTab::Entities => 1,
        BrushTab::Tracks => 2,
    };
    ui.active_track = active.0;
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
    mut selected: ResMut<SelectedBlockKind>,
    mut tab: ResMut<BrushTab>,
    mut sel_e: ResMut<SelectedEntityKind>,
    mut place_yaw: ResMut<PlaceYaw>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
    storage: Res<LevelStorage>,
    mut overlay: ResMut<OverlayMenu>,
    mut clipboard: ResMut<bevy::clipboard::Clipboard>,
) {
    let commands: Vec<UiCommand> = ui.commands.drain(..).collect();
    for cmd in commands {
        match cmd {
            UiCommand::SelectBlock(kind) => selected.0 = kind,
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
                    Ok(true) => ui.set_status("Loaded"),
                    Ok(false) => ui.set_status("No save found"),
                    Err(e) => ui.set_status(format!("Load failed: {e}")),
                }
            }
            UiCommand::NewLevel => {
                level.seed_default();
                history.undo.clear();
                history.redo.clear();
                ui.set_status("New level");
            }
            UiCommand::SaveAs(name) => match storage::save_level(&storage, &mut level, &name) {
                Ok(()) => {
                    level.data.name = name.clone();
                    ui.set_status(format!("Saved '{name}'"));
                }
                Err(e) => ui.set_status(format!("Save failed: {e}")),
            },
            UiCommand::LoadSlot(name) => {
                match storage::load_level(&storage, &mut level, &mut history, &name) {
                    Ok(true) => ui.set_status(format!("Loaded '{name}'")),
                    Ok(false) => ui.set_status("Slot empty"),
                    Err(e) => ui.set_status(format!("Load failed: {e}")),
                }
            }
            UiCommand::RetryPlay => {
                ui.goal_latched = false;
                ui.play_timer = 0.0;
                ui.deaths = 0;
                *mode = MakerMode::Play;
            }
            UiCommand::RefreshSlotList => {
                ui.level_slots = storage.0.list().unwrap_or_default();
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
                        *overlay = OverlayMenu::None;
                        ui.set_status("Level imported!");
                    }
                    Err(e) => ui.set_status(format!("Bad code: {e}")),
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
        }
    }
    ui.level_slots = storage.0.list().unwrap_or_default();
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
        } else if let Some(text) = &ev.text {
            if text.chars().all(|c| !c.is_control()) {
                ui.import_code.push_str(text);
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
