use std::collections::HashSet;

use bevy_ecs::world::World;
use glam::IVec3;

use super::block::ALL_BLOCK_SHAPES;
use super::entity_data::EntityDataExt;
use super::level::{BlockData, LevelDocument};
use super::mode::{ClipboardBlock, ClipboardEntity, EditorClipboard, SelectionSet};
use super::track::TrackId;

pub fn next_variant(existing: &BlockData) -> BlockData {
    let mut next = existing.clone();
    if !existing.kind.is_thin() && existing.kind != super::block::BlockKind::Water {
        let idx = ALL_BLOCK_SHAPES
            .iter()
            .position(|s| *s == existing.shape)
            .unwrap_or(0);
        let next_shape = ALL_BLOCK_SHAPES[(idx + 1) % ALL_BLOCK_SHAPES.len()];
        if next_shape != existing.shape {
            next.shape = next_shape;
            return next;
        }
    }
    next.rot = (next.rot + 1) % 4;
    next
}

pub fn cell_in_aabb(cell: IVec3, min: IVec3, max: IVec3) -> bool {
    cell.x >= min.x
        && cell.x <= max.x
        && cell.y >= min.y
        && cell.y <= max.y
        && cell.z >= min.z
        && cell.z <= max.z
}

pub fn selection_pivot(level: &LevelDocument, selection: &SelectionSet) -> Option<IVec3> {
    let mut pivot: Option<IVec3> = None;

    for &cell in &selection.blocks {
        pivot = Some(match pivot {
            Some(p) => IVec3::new(p.x.min(cell.x), p.y.min(cell.y), p.z.min(cell.z)),
            None => cell,
        });
    }

    for &id in &selection.entities {
        if let Some(entity) = level.entity_by_id(id) {
            let cell = entity.cell_i();
            pivot = Some(match pivot {
                Some(p) => IVec3::new(p.x.min(cell.x), p.y.min(cell.y), p.z.min(cell.z)),
                None => cell,
            });
        }
    }

    pivot
}

pub fn copy_selection_to_clipboard(
    level: &LevelDocument,
    selection: &SelectionSet,
    clipboard: &mut EditorClipboard,
) -> usize {
    clipboard.clear();

    let Some(pivot) = selection_pivot(level, selection) else {
        return 0;
    };

    for &cell in &selection.blocks {
        if let Some(block) = level.get_block(cell).cloned() {
            clipboard.blocks.push(ClipboardBlock {
                offset: cell - pivot,
                data: block,
            });
        }
    }

    for &id in &selection.entities {
        if let Some(entity) = level.entity_by_id(id).cloned() {
            clipboard.entities.push(ClipboardEntity {
                offset: entity.cell_i() - pivot,
                data: entity,
            });
        }
    }

    clipboard.len()
}

pub fn transformed_cell(offset: IVec3, pivot: IVec3, yaw: f32) -> IVec3 {
    let mut p = offset;
    match yaw as i32 % 360 {
        90 => p = IVec3::new(p.z, p.y, -p.x),
        180 => p = IVec3::new(-p.x, p.y, -p.z),
        270 => p = IVec3::new(-p.z, p.y, p.x),
        _ => {}
    }
    pivot + p
}

pub fn rotate_yaw(yaw: &mut f32) {
    *yaw = (*yaw + 90.0) % 360.0;
}

/// Drop dangling editor references (removed entity ids / track ids) from the
/// selection state. Sequential single-resource borrows so it can run between
/// frame phases without holding two `World` borrows at once.
pub fn validate_refs(world: &mut World) {
    let entity_ids: HashSet<_> = world
        .resource::<LevelDocument>()
        .data
        .entities
        .iter()
        .map(|e| e.id)
        .collect();
    let track_ids: HashSet<TrackId> = world
        .resource::<LevelDocument>()
        .data
        .tracks
        .iter()
        .map(|t| t.id)
        .collect();
    {
        let mut selection = world.resource_mut::<SelectionSet>();
        selection.entities.retain(|id| entity_ids.contains(id));
    }
    {
        let mut selected = world.resource_mut::<super::mode::SelectedEntity>();
        if selected.0.is_some_and(|id| !entity_ids.contains(&id)) {
            selected.0 = None;
        }
    }
    {
        let mut active = world.resource_mut::<super::track::ActiveTrack>();
        if active.0.is_some_and(|id| !track_ids.contains(&id)) {
            active.0 = None;
        }
    }
}
