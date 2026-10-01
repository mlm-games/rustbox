mod action;
mod browser;
mod components;
mod dialogs;
mod home;
mod root;
mod style;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::maker::catalog::{LevelSummary, filter_catalog};
use crate::maker::level::LevelTag;

pub use action::UiAction;
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
}

/// Menu-facing app snapshot: phase, overlays and the local browser state.
/// Trims main's `SharedUi` plus `MakerUi`'s browse fields down to the
/// ported screens (home, local level browser, pause, sign dialog).
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
