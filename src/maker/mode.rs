use std::collections::HashSet;

use bevy::prelude::*;

use super::block::{BlockKind, BlockShape};
use super::entity_data::{EntityData, EntityKind, LevelEntityId};
use super::level::BlockData;

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MakerMode {
    #[default]
    Edit,
    Play,
}

/// The active block brush: kind (material), voxel shape, yaw rotation and
/// whether newly placed blocks should be waterlogged.
#[derive(Resource, Clone, Copy, Debug)]
pub struct BlockBrush {
    pub kind: BlockKind,
    pub shape: BlockShape,
    pub rot: u8,
    pub waterlogged: bool,
}

impl Default for BlockBrush {
    fn default() -> Self {
        Self {
            kind: BlockKind::Grass,
            shape: BlockShape::Full,
            rot: 0,
            waterlogged: false,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BrushTab {
    #[default]
    Blocks,
    Entities,
    Tracks,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct SelectedEntityKind(pub EntityKind);

impl Default for SelectedEntityKind {
    fn default() -> Self {
        Self(EntityKind::Glimmer)
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct PlaceYaw(pub f32);

impl Default for PlaceYaw {
    fn default() -> Self {
        Self(0.0)
    }
}

#[derive(Resource, Default)]
pub struct InputCapture {
    pub ui_wants_pointer: bool,
    pub ui_wants_keyboard: bool,
}

/// The entity currently selected in the inspector (Edit mode only).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SelectedEntity(pub Option<LevelEntityId>);

/// Mirror brush mode: bit 0 = X mirror, bit 1 = Z mirror.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct MirrorMode(pub u8);

/// Editor paint state: `start` is the first corner of an in-progress
/// Shift+click box fill; `last_paint`/`last_erase` track the last cell of a
/// hold-drag.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct BoxFillStart {
    pub start: Option<IVec3>,
    pub last_paint: Option<IVec3>,
    pub last_erase: Option<IVec3>,
    /// Pointer position when we last painted or erased in this stroke.
    pub last_pointer: Option<Vec2>,
}

/// Active link channel stamped onto newly placed orbs/gates (1-9).
#[derive(Resource, Clone, Copy, Debug)]
pub struct ActiveLinkChannel(pub u32);

impl Default for ActiveLinkChannel {
    fn default() -> Self {
        Self(1)
    }
}

/// Cell the edit cursor is currently aiming at (Edit mode only).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct EditorCursor {
    pub hit: Option<IVec3>,
    pub place: Option<IVec3>,
    /// Current screen-space cursor position (for drag-paint gating).
    pub pointer: Option<Vec2>,
}

/// Emitted when a block is placed so the pop-in ghost can spawn.
#[derive(Message, Clone, Copy, Debug)]
pub struct BlockPlaced {
    pub cell: IVec3,
    pub kind: BlockKind,
    pub shape: BlockShape,
    pub rot: u8,
}

#[derive(Resource, Default)]
pub struct MakerStats {
    pub blocks_placed: u32,
}

/// Multi-selection used by build-mode structure editing.
#[derive(Resource, Default, Clone, Debug)]
pub struct SelectionSet {
    pub blocks: HashSet<IVec3>,
    pub entities: HashSet<LevelEntityId>,
}

impl SelectionSet {
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.entities.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.entities.is_empty()
    }

    pub fn len(&self) -> usize {
        self.blocks.len() + self.entities.len()
    }

    pub fn toggle_block(&mut self, cell: IVec3) {
        if !self.blocks.remove(&cell) {
            self.blocks.insert(cell);
        }
    }

    pub fn toggle_entity(&mut self, id: LevelEntityId) {
        if !self.entities.remove(&id) {
            self.entities.insert(id);
        }
    }
}

/// First corner for two-click volume selection.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SelectionBoxStart {
    pub start: Option<IVec3>,
}

/// One copied block, stored relative to the clipboard pivot.
#[derive(Clone, Debug)]
pub struct ClipboardBlock {
    pub offset: IVec3,
    pub data: BlockData,
}

/// One copied entity, stored relative to the clipboard pivot.
#[derive(Clone, Debug)]
pub struct ClipboardEntity {
    pub offset: IVec3,
    pub data: EntityData,
}

/// Internal editor clipboard for selected structures.
#[derive(Resource, Default, Clone, Debug)]
pub struct EditorClipboard {
    pub blocks: Vec<ClipboardBlock>,
    pub entities: Vec<ClipboardEntity>,
}

impl EditorClipboard {
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.entities.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.entities.is_empty()
    }

    pub fn len(&self) -> usize {
        self.blocks.len() + self.entities.len()
    }
}

/// Live preview of a clipboard structure while the user positions it.
#[derive(Resource, Default, Debug)]
pub struct PastePreview {
    pub active: bool,
    pub clipboard: EditorClipboard,
    pub current_pivot: IVec3,
    pub yaw: f32, // 0, 90, 180, 270
}

impl PastePreview {
    pub fn reset(&mut self) {
        self.active = false;
        self.clipboard.clear();
        self.yaw = 0.0;
    }
}
