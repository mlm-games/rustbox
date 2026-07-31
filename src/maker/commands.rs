use bevy::prelude::*;

use super::block::BlockKind;
use super::entity_data::EntityData;
use super::level::LevelDocument;
use super::track::{TrackData, TrackId, TrackMode};

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
    CreateTrack {
        track: TrackData,
    },
    DeleteTrack {
        track: TrackData,
    },
    AddTrackPoint {
        track_id: TrackId,
        index: usize,
        cell: [i32; 3],
    },
    RemoveTrackPoint {
        track_id: TrackId,
        index: usize,
        cell: [i32; 3],
    },
    SetTrackMode {
        track_id: TrackId,
        old: TrackMode,
        new: TrackMode,
    },
    SetTrackSpeed {
        track_id: TrackId,
        old: f32,
        new: f32,
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
        EditCommand::Place { position, kind, .. } => level.set_block(*position, Some(*kind)),
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
        EditCommand::CreateTrack { track } => {
            level.add_track(track.clone());
        }
        EditCommand::DeleteTrack { .. } => {}
        EditCommand::AddTrackPoint {
            track_id,
            index,
            cell,
        } => {
            if let Some(t) = level.track_mut(*track_id) {
                let index = (*index).min(t.points.len());
                t.points.insert(index, *cell);
            }
        }
        EditCommand::RemoveTrackPoint {
            track_id, index, ..
        } => {
            if let Some(t) = level.track_mut(*track_id) {
                if *index < t.points.len() {
                    t.points.remove(*index);
                }
            }
        }
        EditCommand::SetTrackMode { track_id, new, .. } => {
            if let Some(t) = level.track_mut(*track_id) {
                t.mode = *new;
            }
        }
        EditCommand::SetTrackSpeed { track_id, new, .. } => {
            if let Some(t) = level.track_mut(*track_id) {
                t.speed = *new;
            }
        }
    }
    level.rebuild_blocks_vec();
}

pub fn revert_command(level: &mut LevelDocument, cmd: &EditCommand) {
    match cmd {
        EditCommand::Place {
            position, previous, ..
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
        EditCommand::CreateTrack { .. } => {
            level.remove_track(cmd_track_id(cmd));
        }
        EditCommand::DeleteTrack { track } => {
            level.add_track(track.clone());
        }
        EditCommand::AddTrackPoint {
            track_id, index, ..
        } => {
            if let Some(t) = level.track_mut(*track_id) {
                if *index < t.points.len() {
                    t.points.remove(*index);
                }
            }
        }
        EditCommand::RemoveTrackPoint {
            track_id,
            index,
            cell,
        } => {
            if let Some(t) = level.track_mut(*track_id) {
                let index = (*index).min(t.points.len());
                t.points.insert(index, *cell);
            }
        }
        EditCommand::SetTrackMode { track_id, old, .. } => {
            if let Some(t) = level.track_mut(*track_id) {
                t.mode = *old;
            }
        }
        EditCommand::SetTrackSpeed { track_id, old, .. } => {
            if let Some(t) = level.track_mut(*track_id) {
                t.speed = *old;
            }
        }
    }
    level.rebuild_blocks_vec();
}

fn cmd_track_id(cmd: &EditCommand) -> TrackId {
    match cmd {
        EditCommand::CreateTrack { track } => track.id,
        _ => 0,
    }
}
