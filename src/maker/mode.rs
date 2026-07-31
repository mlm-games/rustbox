use bevy::prelude::*;

use super::block::BlockKind;
use super::entity_data::{EntityKind, LevelEntityId};

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MakerMode {
    #[default]
    Edit,
    Play,
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct SelectedBlockKind(pub BlockKind);

impl Default for SelectedBlockKind {
    fn default() -> Self {
        Self(BlockKind::Grass)
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

/// First corner of an in-progress Shift+click box fill.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct BoxFillStart(pub Option<IVec3>);

/// Cell the edit cursor is currently aiming at (Edit mode only).
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct EditorCursor {
    pub hit: Option<IVec3>,
    pub place: Option<IVec3>,
}

/// Emitted when a block is placed so the pop-in ghost can spawn.
#[derive(Message, Clone, Copy, Debug)]
pub struct BlockPlaced {
    pub cell: IVec3,
    pub kind: BlockKind,
}

#[derive(Resource, Default)]
pub struct MakerStats {
    pub blocks_placed: u32,
}
