mod action;
mod browser;
mod components;
mod dialogs;
mod editor;
mod home;
pub mod icons;
mod root;
mod style;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::maker::catalog::{LevelSummary, filter_catalog};
use crate::maker::entity_data::EntityData;
use crate::maker::level::LevelTag;
use crate::maker::track::TrackData;

pub use action::UiAction;
pub use editor::{ingame_hud, part_picker};
pub use root::compose_root;

pub type ActionQueue = Arc<Mutex<Vec<UiAction>>>;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum AppState {
    #[default]
    Splash,
    Loading,
    Title,
    InGame,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayMenu {
    #[default]
    None,
    Pause,
    Browse,
    LoadLevel,
    PartPicker,
}

/// Menu-facing app snapshot: phase, overlays, the local level browser and the
/// maker HUD state (toolbar, stats, inspector selection, sign editor).
#[derive(Default)]
pub struct MenuState {
    pub phase: AppState,
    pub overlay: OverlayMenu,
    pub loading_progress: f32,
    pub maker_mode_edit: bool,
    pub keyboard_captured: bool,
    pub sign_dialog_open: bool,
    pub sign_dialog_lines: Vec<String>,
    pub translations: HashMap<String, String>,
    // Maker toolbar / HUD
    pub maker_status: String,
    pub level_name: String,
    pub level_slots: Vec<String>,
    pub limit_blocks: u32,
    pub limit_entities: u32,
    pub limit_tracks: u32,
    pub limit_vertices: u32,
    pub limit_warning: bool,
    pub limit_over: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub brush_tab: u8,
    pub selected_block: u8,
    pub selected_entity: u8,
    pub brush_shape: u8,
    pub brush_rot: u8,
    pub waterlogged: bool,
    pub mirror: u8,
    pub link_channel: u32,
    pub block_icon_handles: Vec<u64>,
    pub entity_icon_handles: Vec<u64>,
    // Inspector selection snapshots
    pub selected_entity_data: Option<EntityData>,
    pub active_track_data: Option<TrackData>,
    // Sign text editor (Edit mode): id/text track the open field.
    pub sign_editor_open: bool,
    pub sign_editor_id: u32,
    pub sign_editor_text: String,
    // Live play stats
    pub play_time_secs: f32,
    pub deaths: u32,
    pub glimmers_collected: u32,
    pub glimmers_total: u32,
    pub player_armor: u8,
    pub player_keys: [u8; 10],
    // Local level browser
    pub browse_levels: Vec<LevelSummary>,
    pub browse_visible: Vec<LevelSummary>,
    pub browse_query: String,
    pub browse_include_tags: Vec<LevelTag>,
    pub browse_verified_only: bool,
    pub browse_difficulty: Option<u8>,
    pub browse_sort: u8,
    pub browse_confirm_delete: Option<String>,
    pub browse_selected: Option<String>,
    pub browse_cursor: usize,
}

impl MenuState {
    fn filtered(&self) -> Vec<LevelSummary> {
        filter_catalog(
            &self.browse_levels,
            &self.browse_query,
            &self.browse_include_tags,
            self.browse_verified_only,
            self.browse_difficulty,
            self.browse_sort,
        )
    }

    /// Keep the local browse cursor and selection consistent after:
    /// - mouse selection
    /// - query/filter/sort changes
    /// - catalog rebuilds/deletes
    pub fn reconcile_browse_nav(&mut self) {
        let visible = self.filtered();
        if visible.is_empty() {
            self.browse_visible = visible;
            self.browse_cursor = 0;
            self.browse_selected = None;
            self.browse_confirm_delete = None;
            return;
        }
        if let Some(sel) = self.browse_selected.as_deref()
            && let Some(pos) = visible.iter().position(|s| s.key == sel)
        {
            self.browse_cursor = pos;
            self.browse_visible = visible;
            return;
        }
        self.browse_cursor = self.browse_cursor.min(visible.len() - 1);
        if self.browse_selected.is_some() {
            self.browse_selected = Some(visible[self.browse_cursor].key.clone());
        }
        self.browse_visible = visible;
    }
}
