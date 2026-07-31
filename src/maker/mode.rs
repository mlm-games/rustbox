use bevy::prelude::*;

use super::block::BlockKind;
use super::entity_data::EntityKind;

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

#[derive(Resource, Default)]
pub struct MakerStats {
    pub blocks_placed: u32,
}
