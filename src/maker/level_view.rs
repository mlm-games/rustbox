use std::collections::HashMap;

use glam::{IVec3, Vec3};
use repame_view3d::{
    CHUNK_SIZE, Cell, ChunkCache, ChunkMeshInput, ChunkMeshOutput, ChunkStreamer, FaceKind,
    Frame3d, MeshGroup, OrbitCamera, Rgb, View3dEvent, VoxelShape, VoxelSource, build_chunk_mesh,
};

use super::block::{BlockKind, BlockKindColor, BlockShape};
use super::commands::{
    CommandHistory, EditCommand, apply_commands_immediate, build_block_data, detached_for,
    mirror_cells, mirror_rot_for, place_cmd_for_cell_with_rot, remove_cmd_for_cell, same_block,
};
use super::edit_ops::{next_variant, transformed_cell};
use super::entity_data::{EntityData, EntityDataExt, EntityKind, LevelEntityId};
use super::level::{BlockData, LevelDocument, raycast_present};
use super::limits::LevelLimits;
use super::mode::{
    BlockBrush, BlockPlaced, BoxFillStart, PlaceGhost, SelectedEntity, SelectionSet,
};
use super::track::{ActiveTrack, TrackData, TrackMode};

pub const DEFAULT_TRACK_SPEED: f32 = 2.0;

const PULSE_ON: bool = true;
const TERRAIN_PICK: u32 = 1;

type Classify = fn(u32) -> FaceKind;
type Solidity = fn(u32) -> bool;

pub fn kind_id(kind: BlockKind) -> u32 {
    rustbox_format::ALL_BLOCK_KINDS
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0) as u32
}

pub fn kind_of(id: u32) -> BlockKind {
    rustbox_format::ALL_BLOCK_KINDS
        .get(id as usize)
        .copied()
        .unwrap_or(BlockKind::Grass)
}

fn voxel_shape(shape: BlockShape) -> VoxelShape {
    match shape {
        BlockShape::Full => VoxelShape::Full,
        BlockShape::Half => VoxelShape::Half,
        BlockShape::TopHalf => VoxelShape::TopHalf,
        BlockShape::Slope => VoxelShape::Slope,
        BlockShape::DSlope => VoxelShape::DSlope,
        BlockShape::Corner => VoxelShape::Corner,
        BlockShape::OuterCorner => VoxelShape::OuterCorner,
        BlockShape::VerticalSlope => VoxelShape::VerticalSlope,
        BlockShape::VerticalSlab => VoxelShape::VerticalSlab,
        BlockShape::Thin => VoxelShape::Thin,
    }
}

impl VoxelSource for LevelDocument {
    fn get(&self, cell: [i32; 3]) -> Option<Cell> {
        let block = self.map.get(&IVec3::new(cell[0], cell[1], cell[2]))?;
        Some(Cell {
            kind: kind_id(block.kind),
            shape: voxel_shape(block.shape),
            rot: block.rot & 3,
            waterlogged: block.waterlogged,
        })
    }
}

fn classify(id: u32) -> FaceKind {
    match rustbox_mesh::classify(kind_of(id), PULSE_ON) {
        rustbox_mesh::FaceKind::Empty => FaceKind::Empty,
        rustbox_mesh::FaceKind::Opaque => FaceKind::Opaque,
        rustbox_mesh::FaceKind::Fluid => FaceKind::Fluid,
    }
}

fn is_solid(id: u32) -> bool {
    kind_of(id).is_solid()
}

fn kind_tints() -> HashMap<u32, Rgb> {
    rustbox_format::ALL_BLOCK_KINDS
        .iter()
        .map(|&kind| (kind_id(kind), srgb_to_linear(kind.color())))
        .collect()
}

pub fn srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    [linear(c[0]), linear(c[1]), linear(c[2])]
}

fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

const POP_IN_SECS: f32 = 0.18;
const GHOST_LIFE_SECS: f32 = 0.25;

pub struct LevelView {
    pub history: CommandHistory,
    pub brush: BlockBrush,
    pub last_action: String,
    pub place_events: Vec<BlockPlaced>,
    pub ghosts: Vec<PlaceGhost>,
    stroke: BoxFillStart,
    streamer: ChunkStreamer<LevelDocument, Classify, Solidity>,
    cache: ChunkCache,
    generation: u64,
}

impl LevelView {
    pub fn new(level: &LevelDocument) -> Self {
        let input: ChunkMeshInput<Classify, Solidity> = ChunkMeshInput {
            classify,
            is_solid,
            kind_tint: kind_tints(),
            water_level: level.water_level(),
            lit: true,
            ..Default::default()
        };
        Self {
            streamer: ChunkStreamer::new(level.clone(), input, 2, 512),
            history: CommandHistory::default(),
            brush: BlockBrush::default(),
            last_action: String::new(),
            place_events: Vec::new(),
            ghosts: Vec::new(),
            stroke: BoxFillStart::default(),
            cache: ChunkCache::new(),
            generation: 0,
        }
    }

    pub fn chunk_count(&self) -> usize {
        self.cache.chunk_count()
    }

    pub fn tick(&mut self, level: &mut LevelDocument, focus: Vec3) {
        let dirty = level.drain_dirty_sorted(focus);
        if !dirty.is_empty() {
            self.generation += 1;
            for (rank, chunk) in dirty.iter().enumerate() {
                let key = [chunk.x, chunk.y, chunk.z];
                if self.cache.is_stale(&key, self.generation) {
                    self.streamer.request(key, self.generation, rank as f32);
                }
            }
        }
        let _ = self.streamer.pump(1);
        let (stored, _) = self.streamer.drain_into(&mut self.cache);
        if stored > 0 {
            stamp_pick_id(&mut self.cache);
        }
    }

    pub fn draw(
        &self,
        frame: &mut Frame3d,
        level: &LevelDocument,
        preview: Option<IVec3>,
        in_edit: bool,
    ) {
        frame.extend_chunks(self.cache.draws());
        if !in_edit {
            return;
        }
        if let Some(cell) = preview {
            let data = build_block_data(
                self.brush.kind,
                self.brush.shape,
                self.brush.rot,
                self.brush.waterlogged,
                cell,
            );
            let scale = if level.boundary_solid(cell) {
                0.98
            } else {
                1.02
            };
            frame.push(shaped_group(&data, scale));
        }
        for ghost in &self.ghosts {
            let data = build_block_data(ghost.kind, ghost.shape, ghost.rot, false, ghost.cell);
            frame.push(shaped_group(&data, pop_in_scale(ghost.age)));
        }
    }

    pub fn stroke_click_ready(&self) -> bool {
        self.stroke.last_paint.is_none() && self.stroke.last_erase.is_none()
    }

    pub fn undo(&mut self, level: &mut LevelDocument) {
        if self.history.undo.is_empty() {
            return;
        }
        self.history.undo(level);
        self.sync_source(level);
        self.last_action = "undo".to_string();
    }

    pub fn redo(&mut self, level: &mut LevelDocument) {
        if self.history.redo.is_empty() {
            return;
        }
        self.history.redo(level);
        self.sync_source(level);
        self.last_action = "redo".to_string();
    }

    fn paint_cmds(
        &mut self,
        level: &mut LevelDocument,
        cmds: Vec<EditCommand>,
        place_cell: IVec3,
        limits: &LevelLimits,
        anchor: bool,
    ) {
        let net_new = cmds
            .iter()
            .filter(|c| matches!(c, EditCommand::Place { previous: None, .. }))
            .count();
        if !cmds.is_empty() && (level.map.len() + net_new) <= limits.max_blocks as usize {
            for cmd in &cmds {
                if let EditCommand::Place { position, data, .. } = cmd {
                    self.place_events.push(BlockPlaced {
                        cell: *position,
                        kind: data.kind,
                        shape: data.shape,
                        rot: data.rot,
                    });
                }
            }
            apply_commands_immediate(level, &cmds);
            self.stroke.stroke.extend(cmds);
            self.sync_source(level);
            self.last_action = format!(
                "place {:?} at {}, {}, {}",
                self.brush.kind, place_cell.x, place_cell.y, place_cell.z
            );
        }
        if anchor {
            self.stroke.last_paint = Some(place_cell);
        }
    }

    pub fn stroke_paint(
        &mut self,
        level: &mut LevelDocument,
        mirror: u8,
        place_cell: IVec3,
        limits: &LevelLimits,
    ) {
        if self.stroke.last_paint == Some(place_cell) {
            return;
        }
        let cmds: Vec<EditCommand> = mirror_cells(place_cell, mirror)
            .into_iter()
            .filter_map(|cell| {
                let rot = mirror_rot_for(place_cell, cell, self.brush.rot);
                place_cmd_for_cell_with_rot(level, &self.brush, cell, rot)
            })
            .collect();
        self.paint_cmds(level, cmds, place_cell, limits, true);
    }

    pub fn click_place(
        &mut self,
        level: &mut LevelDocument,
        mirror: u8,
        place_cell: IVec3,
        limits: &LevelLimits,
    ) {
        if self.stroke.last_paint == Some(place_cell) {
            return;
        }
        let mirrored = mirror_cells(place_cell, mirror);
        let should_cycle = mirrored.iter().any(|c| {
            level
                .get_block(*c)
                .is_some_and(|b| b.kind == self.brush.kind)
        });
        let cmds: Vec<EditCommand> = if should_cycle {
            mirrored
                .into_iter()
                .filter_map(|cell| {
                    let rot = mirror_rot_for(place_cell, cell, self.brush.rot);
                    let Some(existing) = level.get_block(cell) else {
                        return place_cmd_for_cell_with_rot(level, &self.brush, cell, rot);
                    };
                    if existing.kind != self.brush.kind {
                        return place_cmd_for_cell_with_rot(level, &self.brush, cell, rot);
                    }
                    if level.boundary_solid(cell) {
                        return None;
                    }
                    let mut next = next_variant(existing);
                    next.position = cell.to_array();
                    if same_block(existing, &next) {
                        return None;
                    }
                    Some(EditCommand::Place {
                        position: cell,
                        data: next,
                        previous: Some(existing.clone()),
                    })
                })
                .collect()
        } else {
            mirrored
                .into_iter()
                .filter_map(|cell| {
                    let rot = mirror_rot_for(place_cell, cell, self.brush.rot);
                    place_cmd_for_cell_with_rot(level, &self.brush, cell, rot)
                })
                .collect()
        };
        self.paint_cmds(level, cmds, place_cell, limits, true);
    }

    pub fn stroke_erase(&mut self, level: &mut LevelDocument, mirror: u8, hit_cell: IVec3) {
        if self.stroke.last_erase == Some(hit_cell) {
            return;
        }
        let cmds: Vec<EditCommand> = mirror_cells(hit_cell, mirror)
            .into_iter()
            .filter_map(|cell| remove_cmd_for_cell(level, cell))
            .collect();
        if !cmds.is_empty() {
            apply_commands_immediate(level, &cmds);
            self.stroke.stroke.extend(cmds);
            self.sync_source(level);
            self.last_action = format!("erase at {}, {}, {}", hit_cell.x, hit_cell.y, hit_cell.z);
        }
        self.stroke.last_erase = Some(hit_cell);
    }

    pub fn box_fill(
        &mut self,
        level: &mut LevelDocument,
        a: IVec3,
        b: IVec3,
        limits: &LevelLimits,
    ) -> usize {
        let min = a.min(b);
        let max = a.max(b);
        let vol =
            (max.x - min.x + 1) as u64 * (max.y - min.y + 1) as u64 * (max.z - min.z + 1) as u64;
        if vol > 1_000_000 {
            return 0;
        }
        let room = limits.max_blocks.saturating_sub(level.map.len() as u32);
        let mut cells: Vec<(IVec3, Option<BlockData>)> = Vec::new();
        let mut new_count: u32 = 0;
        let mut over_budget = false;
        'scan: for x in min.x..=max.x {
            for y in min.y..=max.y {
                for z in min.z..=max.z {
                    let cell = IVec3::new(x, y, z);
                    let prev = level.get_block(cell).cloned();
                    let same = prev.as_ref().is_some_and(|p| {
                        (p.kind, p.shape, p.rot, p.waterlogged)
                            == (
                                self.brush.kind,
                                self.brush.shape,
                                self.brush.rot,
                                self.brush.waterlogged,
                            )
                    });
                    if same {
                        continue;
                    }
                    if prev.is_none() {
                        new_count += 1;
                        if new_count > room {
                            over_budget = true;
                            break 'scan;
                        }
                    }
                    cells.push((cell, prev));
                }
            }
        }
        if over_budget || cells.is_empty() {
            return 0;
        }
        let count = cells.len();
        let data = build_block_data(
            self.brush.kind,
            self.brush.shape,
            self.brush.rot,
            self.brush.waterlogged,
            b,
        );
        self.history
            .apply(level, EditCommand::BoxFill { cells, data });
        self.sync_source(level);
        self.last_action = format!("box fill {count} block(s)");
        count
    }

    pub fn place_entity(
        &mut self,
        level: &mut LevelDocument,
        cell: IVec3,
        kind: EntityKind,
        yaw: f32,
        channel: u32,
        limits: &LevelLimits,
    ) -> bool {
        if (level.data.entities.len() as u32) >= limits.max_entities
            || !level.can_place_entity_at(cell, kind)
        {
            return false;
        }
        let id = level.alloc_id();
        let mut data = EntityData::defaults_for(kind, cell, id);
        data.yaw_deg = yaw;
        if data.kind.uses_link() {
            data.link = channel;
        }
        if data.kind == EntityKind::Cannon {
            let yaw = yaw.to_radians();
            let dir = IVec3::new(
                (yaw.sin() * 4.0).round() as i32,
                0,
                (yaw.cos() * 4.0).round() as i32,
            );
            data.cell_b = Some((cell + dir).to_array());
        }
        let world = cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
        data.track = level.track_near(world, 1.5);
        self.history
            .apply(level, EditCommand::PlaceEntity { entity: data });
        self.last_action = format!("place {:?}", kind);
        true
    }

    pub fn erase_at(
        &mut self,
        level: &mut LevelDocument,
        mirror: u8,
        hit: IVec3,
        selected: &mut SelectedEntity,
    ) {
        if level.get_block(hit).is_some() {
            self.stroke_erase(level, mirror, hit);
            return;
        }
        let Some(entity) = level.top_entity_at_cell(hit).cloned() else {
            return;
        };
        let id = entity.id;
        self.history
            .apply(level, EditCommand::RemoveEntity { entity });
        if selected.0 == Some(id) {
            selected.0 = None;
        }
        self.last_action = "remove entity".to_string();
    }

    pub fn track_place_click(
        &mut self,
        level: &mut LevelDocument,
        place: IVec3,
        active: &mut ActiveTrack,
        limits: &LevelLimits,
    ) {
        if let Some(id) = level.track_at_cell(place) {
            active.0 = Some(id);
            self.last_action = "track selected".to_string();
            return;
        }
        if let Some(id) = active.0.filter(|id| level.track(*id).is_some()) {
            let last = level.track(id).and_then(|t| t.points.last().copied());
            if last != Some(place.to_array()) {
                let index = level.track(id).map(|t| t.points.len()).unwrap_or(0);
                self.history.apply(
                    level,
                    EditCommand::AddTrackPoint {
                        track_id: id,
                        index,
                        cell: place.to_array(),
                    },
                );
                self.last_action = "track point added".to_string();
            }
            return;
        }
        if (level.data.tracks.len() as u32) >= limits.max_tracks {
            return;
        }
        let id = level.alloc_track_id();
        let track = TrackData {
            id,
            points: vec![place.to_array()],
            mode: TrackMode::default(),
            speed: DEFAULT_TRACK_SPEED,
        };
        self.history
            .apply(level, EditCommand::CreateTrack { track });
        active.0 = Some(id);
        self.last_action = "track created".to_string();
    }

    pub fn track_erase_click(
        &mut self,
        level: &mut LevelDocument,
        place: IVec3,
        active: &mut ActiveTrack,
    ) {
        if let Some(id) = active.0 {
            let on_waypoint = level.track_at_cell(place) == Some(id);
            let (index, len) = if on_waypoint {
                match level.track(id) {
                    Some(t) => match t.points.iter().position(|p| IVec3::from_array(*p) == place) {
                        Some(i) => (i, t.points.len()),
                        None => (0, 0),
                    },
                    None => (0, 0),
                }
            } else {
                let len = level.track(id).map(|t| t.points.len()).unwrap_or(0);
                if len > 0 { (len - 1, len) } else { (0, 0) }
            };
            if len == 0 {
                return;
            }
            if len <= 1 {
                if let Some(track) = level.track(id).cloned() {
                    let detached = detached_for(level, id);
                    self.history
                        .apply(level, EditCommand::DeleteTrack { track, detached });
                }
                active.0 = None;
                self.last_action = "track deleted".to_string();
            } else if let Some(cell) = level.track(id).and_then(|t| t.points.get(index).copied()) {
                self.history.apply(
                    level,
                    EditCommand::RemoveTrackPoint {
                        track_id: id,
                        index,
                        cell,
                    },
                );
                self.last_action = "track point removed".to_string();
            }
        } else if let Some(id) = level.track_at_cell(place) {
            active.0 = Some(id);
            self.last_action = "track selected".to_string();
        }
    }

    pub fn delete_selection(
        &mut self,
        level: &mut LevelDocument,
        selection: &mut SelectionSet,
        selected: &mut SelectedEntity,
    ) -> usize {
        let mut blocks = Vec::new();
        let mut entities = Vec::new();
        for &cell in &selection.blocks {
            if let Some(block) = level.get_block(cell).cloned() {
                blocks.push((cell, block));
            }
        }
        for &id in &selection.entities {
            if let Some(entity) = level.entity_by_id(id).cloned() {
                entities.push(entity);
            }
        }
        let count = blocks.len() + entities.len();
        if count == 0 {
            selection.clear();
            selected.0 = None;
            return 0;
        }
        self.history
            .apply(level, EditCommand::DeleteSelection { blocks, entities });
        selection.clear();
        selected.0 = None;
        self.sync_source(level);
        self.last_action = format!("delete {count} item(s)");
        count
    }

    #[allow(clippy::too_many_arguments)]
    pub fn paste_clipboard(
        &mut self,
        level: &mut LevelDocument,
        selection: &mut SelectionSet,
        selected: &mut SelectedEntity,
        clipboard: &super::mode::EditorClipboard,
        target: IVec3,
        yaw: f32,
        limits: &LevelLimits,
    ) -> usize {
        if clipboard.is_empty() {
            return 0;
        }
        let mut blocks: Vec<(IVec3, BlockData, Option<BlockData>)> = Vec::new();
        let mut entities: Vec<EntityData> = Vec::new();
        for item in &clipboard.blocks {
            let pos = transformed_cell(item.offset, target, yaw);
            if pos.x.abs() > 512 || pos.y.abs() > 512 || pos.z.abs() > 512 {
                continue;
            }
            if level.boundary_solid(pos) {
                continue;
            }
            let mut data = item.data.clone();
            data.position = pos.to_array();
            data.rot = (data.rot + ((yaw as i32 / 90).rem_euclid(4) as u8)) % 4;
            let previous = level.get_block(pos).cloned();
            blocks.push((pos, data, previous));
        }
        for item in &clipboard.entities {
            let pos = transformed_cell(item.offset, target, yaw);
            if pos.x.abs() > 512 || pos.y.abs() > 512 || pos.z.abs() > 512 {
                continue;
            }
            if level.boundary_solid(pos) || !level.can_place_entity_at(pos, item.data.kind) {
                continue;
            }
            let old_cell = item.data.cell_i();
            let mut entity = item.data.clone();
            entity.id = level.alloc_id();
            entity.cell = pos.to_array();
            entity.yaw_deg = (entity.yaw_deg + yaw) % 360.0;
            if let Some(cell_b) = entity.cell_b {
                let relative_b = IVec3::from_array(cell_b) - old_cell;
                let rotated_b = transformed_cell(relative_b, IVec3::ZERO, yaw);
                entity.cell_b = Some((pos + rotated_b).to_array());
            }
            entities.push(entity);
        }
        if blocks.is_empty() && entities.is_empty() {
            return 0;
        }
        let net_new = blocks.iter().filter(|(_, _, prev)| prev.is_none()).count() as u32;
        let room = limits.max_blocks.saturating_sub(level.map.len() as u32);
        if net_new > room {
            let mut keep = room;
            let mut trimmed = Vec::new();
            for block in blocks {
                let is_new = block.2.is_none();
                if is_new && keep == 0 {
                    continue;
                }
                if is_new {
                    keep -= 1;
                }
                trimmed.push(block);
            }
            blocks = trimmed;
        }
        let entity_room = (limits.max_entities as usize).saturating_sub(level.data.entities.len());
        entities.truncate(entity_room);
        if blocks.is_empty() && entities.is_empty() {
            return 0;
        }
        let pasted_blocks: Vec<IVec3> = blocks.iter().map(|(pos, _, _)| *pos).collect();
        let pasted_entities: Vec<LevelEntityId> = entities.iter().map(|e| e.id).collect();
        let count = pasted_blocks.len() + pasted_entities.len();
        self.history
            .apply(level, EditCommand::PasteSelection { blocks, entities });
        selection.clear();
        for cell in pasted_blocks {
            selection.blocks.insert(cell);
        }
        for id in pasted_entities {
            selection.entities.insert(id);
        }
        selected.0 = selection.entities.iter().next().copied();
        self.sync_source(level);
        self.last_action = format!("paste {count} item(s)");
        count
    }

    pub fn release_stroke(&mut self, level: &mut LevelDocument) {
        self.stroke.last_paint = None;
        self.stroke.last_erase = None;
        self.stroke.last_pointer = None;
        if !self.stroke.stroke.is_empty() {
            let cmds = std::mem::take(&mut self.stroke.stroke);
            self.history.apply_many(level, cmds);
        }
    }

    pub fn take_place_events(&mut self) -> Vec<BlockPlaced> {
        std::mem::take(&mut self.place_events)
    }

    pub fn pump_ghosts(&mut self, dt: f32) {
        for ev in self.take_place_events() {
            self.ghosts.push(PlaceGhost {
                cell: ev.cell,
                kind: ev.kind,
                shape: ev.shape,
                rot: ev.rot,
                age: 0.0,
            });
        }
        for ghost in &mut self.ghosts {
            ghost.age += dt;
        }
        self.ghosts.retain(|ghost| ghost.age < GHOST_LIFE_SECS);
    }

    fn sync_source(&mut self, level: &LevelDocument) {
        self.streamer.set_source(level.clone());
    }
}

pub fn resolve_click(
    level: &LevelDocument,
    ev: &View3dEvent,
    cam: &OrbitCamera,
) -> Option<(IVec3, IVec3)> {
    let eye = cam.eye();
    let point = match ev {
        View3dEvent::MeshClick { point, .. } => Vec3::from_array(*point),
        View3dEvent::GroundClick { x, z } => Vec3::new(*x, 0.0, *z),
        _ => return None,
    };
    let dir = point - eye;
    if dir == Vec3::ZERO {
        return None;
    }
    raycast_present(level, eye, dir, 200.0)
        .filter(|(_, normal)| *normal != IVec3::ZERO)
        .or_else(|| {
            raycast_present(level, point - dir.normalize() * 0.5, dir, 4.0)
                .filter(|(_, normal)| *normal != IVec3::ZERO)
        })
}

fn pop_in_scale(age: f32) -> f32 {
    if age >= POP_IN_SECS {
        return 1.0;
    }
    let t = (age / POP_IN_SECS).clamp(0.0, 1.0);
    let t2 = t - 1.0;
    t2 * t2 * ((1.70158 + 1.0) * t2 + 1.70158) + 1.0
}

fn shaped_group(data: &BlockData, scale: f32) -> MeshGroup {
    let cell = IVec3::from_array(data.position);
    let mut grid = HashMap::new();
    grid.insert(
        data.position,
        Cell {
            kind: kind_id(data.kind),
            shape: voxel_shape(data.shape),
            rot: data.rot & 3,
            waterlogged: false,
        },
    );
    let input: ChunkMeshInput<Classify, Solidity> = ChunkMeshInput {
        classify,
        is_solid,
        kind_tint: kind_tints(),
        water_level: None,
        lit: true,
        ..Default::default()
    };
    let mut output = ChunkMeshOutput::default();
    build_chunk_mesh(
        &grid,
        cell.div_euclid(IVec3::splat(CHUNK_SIZE)).to_array(),
        &input,
        &mut output,
    );
    let center = cell.as_vec3() + Vec3::splat(0.5);
    let water = output.water;
    let mut merged = MeshGroup {
        depth_test: true,
        transparent: true,
        alpha: 0.45,
        ..MeshGroup::default()
    };
    for group in output
        .opaque
        .into_values()
        .chain((!water.is_empty()).then_some(water))
    {
        let base = merged.positions.len() as u32;
        for pos in &group.positions {
            let v = Vec3::from_array(*pos);
            merged
                .positions
                .push((center + (v - center) * scale).to_array());
        }
        merged.colors.extend_from_slice(&group.colors);
        merged.normals.extend_from_slice(&group.normals);
        merged.uvs.extend_from_slice(&group.uvs);
        merged
            .indices
            .extend(group.indices.iter().map(|i| base + i));
    }
    merged
}

fn stamp_pick_id(cache: &mut ChunkCache) {
    let fixes = {
        let draws = cache.draws();
        let mut fixes: Vec<([i32; 3], u64, Vec<MeshGroup>)> = Vec::new();
        for draw in &draws {
            if draw.group.pick_id != 0 || fixes.iter().any(|f| f.0 == draw.chunk) {
                continue;
            }
            let Some(generation) = cache.generation(&draw.chunk) else {
                continue;
            };
            let groups = draws
                .iter()
                .filter(|d| d.chunk == draw.chunk)
                .map(|d| MeshGroup {
                    pick_id: TERRAIN_PICK,
                    ..d.group.clone()
                })
                .collect();
            fixes.push((draw.chunk, generation, groups));
        }
        fixes
    };
    for (chunk, generation, groups) in fixes {
        cache.store(chunk, generation, &groups);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_queues_ghost_events_and_ghost_expires_at_quarter_second() {
        let mut level = LevelDocument::default();
        let mut view = LevelView::new(&level);
        view.stroke_paint(&mut level, 0, IVec3::new(0, 1, 0), &LevelLimits::default());
        let events = view.take_place_events();
        assert_eq!(events.len(), 1, "{events:?}");
        assert_eq!(events[0].cell, IVec3::new(0, 1, 0));
        assert_eq!(events[0].kind, BlockKind::Grass);
        assert_eq!(events[0].shape, BlockShape::Full);
        assert_eq!(events[0].rot, 0);
        assert!(view.take_place_events().is_empty());

        assert_eq!(pop_in_scale(0.0), 0.0);
        let mid = pop_in_scale(0.09);
        assert!(mid > 1.08 && mid < 1.09, "{mid}");
        assert_eq!(pop_in_scale(0.18), 1.0);
        assert_eq!(pop_in_scale(0.3), 1.0);

        view.place_events = events;
        view.pump_ghosts(0.0);
        assert_eq!(view.ghosts.len(), 1);
        assert_eq!(view.ghosts[0].age, 0.0);
        view.pump_ghosts(0.24);
        assert_eq!(view.ghosts.len(), 1, "alive at 0.24s");
        view.pump_ghosts(0.02);
        assert!(view.ghosts.is_empty(), "dead past 0.25s");
    }

    #[test]
    fn two_cell_stroke_is_one_undo_step() {
        let mut level = LevelDocument::default();
        let mut view = LevelView::new(&level);
        view.stroke_paint(&mut level, 0, IVec3::new(0, 1, 0), &LevelLimits::default());
        view.stroke_paint(&mut level, 0, IVec3::new(1, 1, 0), &LevelLimits::default());
        assert!(view.history.undo.is_empty(), "stroke defers the undo entry");
        assert!(level.get_block(IVec3::new(0, 1, 0)).is_some());
        assert!(level.get_block(IVec3::new(1, 1, 0)).is_some());
        assert!(view.stroke.last_paint.is_some());

        view.release_stroke(&mut level);
        assert_eq!(view.history.undo.len(), 1, "one undo per stroke");
        assert!(view.stroke.last_paint.is_none());
        assert!(view.stroke.stroke.is_empty());

        view.undo(&mut level);
        assert!(level.get_block(IVec3::new(0, 1, 0)).is_none());
        assert!(level.get_block(IVec3::new(1, 1, 0)).is_none());
    }
}
