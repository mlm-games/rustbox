use bevy::prelude::*;

use super::entity_data::{ContainedItem, EntityData, LevelEntityId};
use super::level::{BlockData, LevelDocument};
use super::track::{TrackData, TrackId, TrackMode};

#[derive(Clone, Debug)]
pub enum EditCommand {
    Place {
        position: IVec3,
        data: BlockData,
        previous: Option<BlockData>,
    },
    Remove {
        position: IVec3,
        previous: BlockData,
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
    SetEntityParam {
        id: LevelEntityId,
        old: f32,
        new: f32,
    },
    SetEntityYaw {
        id: LevelEntityId,
        old: f32,
        new: f32,
    },
    SetEntityTrack {
        id: LevelEntityId,
        old: Option<TrackId>,
        new: Option<TrackId>,
    },
    SetEntityLink {
        id: LevelEntityId,
        old: u32,
        new: u32,
    },
    SetEntityContents {
        id: LevelEntityId,
        old: ContainedItem,
        new: ContainedItem,
    },
    ReverseTrackPoints {
        track_id: TrackId,
    },
    BoxFill {
        cells: Vec<(IVec3, Option<BlockData>)>,
        data: BlockData,
    },
    PasteSelection {
        /// `(position, new_data, previous_data)`
        blocks: Vec<(IVec3, BlockData, Option<BlockData>)>,
        entities: Vec<EntityData>,
    },
    DeleteSelection {
        blocks: Vec<(IVec3, BlockData)>,
        entities: Vec<EntityData>,
    },
}

#[derive(Resource, Default)]
pub struct CommandHistory {
    pub undo: Vec<EditCommand>,
    pub redo: Vec<EditCommand>,
}

/// Any edit invalidates the author clear: a change means the level may no
/// longer be beatable exactly as previously proven.
pub fn invalidate_verification(level: &mut LevelDocument) {
    level.data.is_verified = false;
    level.data.author_time = None;
    level.data.author_deaths = 0;
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
        EditCommand::Place { position, data, .. } => level.set_block(*position, Some(data.clone())),
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
        EditCommand::DeleteTrack { track } => {
            level.remove_track(track.id);
        }
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
            if let Some(t) = level.track_mut(*track_id)
                && *index < t.points.len()
            {
                t.points.remove(*index);
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
        EditCommand::SetEntityParam { id, new, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.param = *new;
            }
        }
        EditCommand::SetEntityYaw { id, new, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.yaw_deg = *new;
            }
        }
        EditCommand::SetEntityTrack { id, new, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.track = *new;
            }
        }
        EditCommand::SetEntityLink { id, new, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.link = *new;
            }
        }
        EditCommand::SetEntityContents { id, new, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.contents = *new;
            }
        }
        EditCommand::ReverseTrackPoints { track_id } => {
            if let Some(t) = level.track_mut(*track_id) {
                t.points.reverse();
            }
        }
        EditCommand::BoxFill { cells, data } => {
            for (pos, _) in cells {
                let mut d = data.clone();
                d.position = pos.to_array();
                level.set_block(*pos, Some(d));
            }
        }
        EditCommand::PasteSelection { blocks, entities } => {
            for (pos, data, _) in blocks {
                let mut d = data.clone();
                d.position = pos.to_array();
                level.set_block(*pos, Some(d));
            }

            for entity in entities {
                let mut e = entity.clone();
                if e.id == 0 {
                    e.id = level.alloc_id();
                } else {
                    level.next_entity_id = level.next_entity_id.max(e.id + 1);
                }
                level.add_entity(e);
            }
        }
        EditCommand::DeleteSelection { blocks, entities } => {
            for (pos, _) in blocks {
                level.set_block(*pos, None);
            }

            for entity in entities {
                level.remove_entity(entity.id);
            }
        }
    }
    level.rebuild_blocks_vec();
    invalidate_verification(level);
}

pub fn revert_command(level: &mut LevelDocument, cmd: &EditCommand) {
    match cmd {
        EditCommand::Place {
            position, previous, ..
        } => level.set_block(*position, previous.clone()),
        EditCommand::Remove {
            position, previous, ..
        } => level.set_block(*position, Some(previous.clone())),
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
            if let Some(t) = level.track_mut(*track_id)
                && *index < t.points.len()
            {
                t.points.remove(*index);
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
        EditCommand::SetEntityParam { id, old, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.param = *old;
            }
        }
        EditCommand::SetEntityYaw { id, old, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.yaw_deg = *old;
            }
        }
        EditCommand::SetEntityTrack { id, old, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.track = *old;
            }
        }
        EditCommand::SetEntityLink { id, old, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.link = *old;
            }
        }
        EditCommand::SetEntityContents { id, old, .. } => {
            if let Some(e) = level.entity_mut(*id) {
                e.contents = *old;
            }
        }
        EditCommand::ReverseTrackPoints { track_id } => {
            if let Some(t) = level.track_mut(*track_id) {
                t.points.reverse();
            }
        }
        EditCommand::BoxFill { cells, .. } => {
            for (pos, prev) in cells {
                level.set_block(*pos, prev.clone());
            }
        }
        EditCommand::PasteSelection { blocks, entities } => {
            for (pos, _, previous) in blocks {
                level.set_block(*pos, previous.clone());
            }

            for entity in entities {
                level.remove_entity(entity.id);
            }
        }
        EditCommand::DeleteSelection { blocks, entities } => {
            for (pos, data) in blocks {
                let mut d = data.clone();
                d.position = pos.to_array();
                level.set_block(*pos, Some(d));
            }

            for entity in entities {
                let mut e = entity.clone();
                if e.id == 0 {
                    e.id = level.alloc_id();
                } else {
                    level.next_entity_id = level.next_entity_id.max(e.id + 1);
                }
                level.add_entity(e);
            }
        }
    }
    level.rebuild_blocks_vec();
    invalidate_verification(level);
}

fn cmd_track_id(cmd: &EditCommand) -> TrackId {
    match cmd {
        EditCommand::CreateTrack { track } => track.id,
        _ => 0,
    }
}
