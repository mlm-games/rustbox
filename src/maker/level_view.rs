use std::collections::HashMap;

use glam::{IVec3, Vec3};
use repame_view3d::{
    CHUNK_SIZE, Cell, ChunkCache, ChunkMeshInput, ChunkMeshOutput, ChunkStreamer, FaceKind,
    Frame3d, MeshGroup, OrbitCamera, Rgb, View3dEvent, VoxelShape, VoxelSource, build_chunk_mesh,
};

use super::block::{BlockKind, BlockKindColor, BlockShape};
use super::commands::{
    CommandHistory, EditCommand, apply_commands_immediate, build_block_data, mirror_cells,
    mirror_rot_for, place_cmd_for_cell_with_rot, remove_cmd_for_cell,
};
use super::level::{BlockData, LevelDocument, raycast_present};
use super::mode::{BlockBrush, BlockPlaced, BoxFillStart, PlaceGhost};

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

    pub fn click(
        &mut self,
        level: &mut LevelDocument,
        ev: &View3dEvent,
        cam: &OrbitCamera,
        erase: bool,
        mirror: u8,
    ) {
        if self.stroke.last_paint.is_some() || self.stroke.last_erase.is_some() {
            return;
        }
        let eye = cam.eye();
        let point = match ev {
            View3dEvent::MeshClick { point, .. } => Vec3::from_array(*point),
            View3dEvent::GroundClick { x, z } => Vec3::new(*x, 0.0, *z),
            _ => return,
        };
        let dir = point - eye;
        if dir == Vec3::ZERO {
            return;
        }
        let hit = raycast_present(level, eye, dir, 200.0)
            .filter(|(_, normal)| *normal != IVec3::ZERO)
            .or_else(|| {
                raycast_present(level, point - dir.normalize() * 0.5, dir, 4.0)
                    .filter(|(_, normal)| *normal != IVec3::ZERO)
            });
        let Some((cell, normal)) = hit else {
            return;
        };
        if erase {
            self.stroke_erase(level, mirror, cell);
        } else {
            self.stroke_paint(level, mirror, cell + normal);
        }
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

    pub fn stroke_paint(&mut self, level: &mut LevelDocument, mirror: u8, place_cell: IVec3) {
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
        if !cmds.is_empty() {
            apply_commands_immediate(level, &cmds);
            self.stroke.stroke.extend(cmds);
            self.sync_source(level);
            self.last_action = format!(
                "place {:?} at {}, {}, {}",
                self.brush.kind, place_cell.x, place_cell.y, place_cell.z
            );
        }
        self.stroke.last_paint = Some(place_cell);
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
        view.stroke_paint(&mut level, 0, IVec3::new(0, 1, 0));
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
        view.stroke_paint(&mut level, 0, IVec3::new(0, 1, 0));
        view.stroke_paint(&mut level, 0, IVec3::new(1, 1, 0));
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
