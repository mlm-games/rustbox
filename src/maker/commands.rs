use bevy::prelude::*;

use super::block::BlockKind;
use super::entity_data::EntityData;
use super::level::LevelDocument;

#[derive(Clone, Debug)]
pub enum EditCommand {
    Place {
        position: IVec3,
        kind: BlockKind,
        previous: Option<BlockKind>,
    },
    Remove {
        position: IVec3,
        previous: BlockKind,
    },
    PlaceEntity {
        entity: EntityData,
    },
    RemoveEntity {
        entity: EntityData,
    },
}

#[derive(Resource, Default)]
pub struct CommandHistory {
    pub undo: Vec<EditCommand>,
    pub redo: Vec<EditCommand>,
}

impl CommandHistory {
    pub fn apply(&mut self, level: &mut LevelDocument, cmd: EditCommand) {
        apply_command(level, &cmd);
        self.undo.push(cmd);
        self.redo.clear();
    }

    pub fn undo(&mut self, level: &mut LevelDocument) {
        let Some(cmd) = self.undo.pop() else {
            return;
        };
        revert_command(level, &cmd);
        self.redo.push(cmd);
    }

    pub fn redo(&mut self, level: &mut LevelDocument) {
        let Some(cmd) = self.redo.pop() else {
            return;
        };
        apply_command(level, &cmd);
        self.undo.push(cmd);
    }
}

pub fn apply_command(level: &mut LevelDocument, cmd: &EditCommand) {
    match cmd {
        EditCommand::Place {
            position, kind, ..
        } => level.set_block(*position, Some(*kind)),
        EditCommand::Remove { position, .. } => level.set_block(*position, None),
        EditCommand::PlaceEntity { entity } => {
            let mut e = entity.clone();
            if e.id == 0 {
                e.id = level.alloc_id();
            } else {
                level.next_entity_id = level.next_entity_id.max(e.id + 1);
            }
            level.add_entity(e);
        }
        EditCommand::RemoveEntity { entity } => {
            level.remove_entity(entity.id);
        }
    }
    level.rebuild_blocks_vec();
}

pub fn revert_command(level: &mut LevelDocument, cmd: &EditCommand) {
    match cmd {
        EditCommand::Place {
            position,
            previous,
            ..
        } => level.set_block(*position, *previous),
        EditCommand::Remove {
            position, previous, ..
        } => level.set_block(*position, Some(*previous)),
        EditCommand::PlaceEntity { entity } => {
            level.remove_entity(entity.id);
        }
        EditCommand::RemoveEntity { entity } => {
            let mut e = entity.clone();
            if e.id == 0 {
                e.id = level.alloc_id();
            } else {
                level.next_entity_id = level.next_entity_id.max(e.id + 1);
            }
            level.add_entity(e);
        }
    }
    level.rebuild_blocks_vec();
}
