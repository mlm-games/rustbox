use std::collections::HashMap;

use glam::{IVec3, Vec3};
use repame_view3d::{
    Cell, ChunkCache, ChunkMeshInput, ChunkStreamer, FaceKind, Frame3d, MeshGroup, OrbitCamera,
    Rgb, View3dEvent, VoxelShape, VoxelSource,
};

use super::block::{BlockKind, BlockKindColor, BlockShape};
use super::commands::{CommandHistory, place_cmd_for_cell, remove_cmd_for_cell};
use super::level::{LevelDocument, raycast_present};
use super::mode::BlockBrush;

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

fn srgb_to_linear(c: [f32; 3]) -> [f32; 3] {
    [linear(c[0]), linear(c[1]), linear(c[2])]
}

fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

pub struct LevelView {
    pub history: CommandHistory,
    pub brush: BlockBrush,
    pub last_action: String,
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

    pub fn draw(&self, frame: &mut Frame3d) {
        frame.extend_chunks(self.cache.draws());
    }

    pub fn click(
        &mut self,
        level: &mut LevelDocument,
        ev: &View3dEvent,
        cam: &OrbitCamera,
        erase: bool,
    ) {
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
            self.erase(level, cell);
        } else {
            self.place(level, cell + normal);
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

    fn place(&mut self, level: &mut LevelDocument, cell: IVec3) {
        let Some(cmd) = place_cmd_for_cell(level, &self.brush, cell) else {
            return;
        };
        self.history.apply(level, cmd);
        self.sync_source(level);
        self.last_action = format!(
            "place {:?} at {}, {}, {}",
            self.brush.kind, cell.x, cell.y, cell.z
        );
    }

    fn erase(&mut self, level: &mut LevelDocument, cell: IVec3) {
        let Some(cmd) = remove_cmd_for_cell(level, cell) else {
            return;
        };
        self.history.apply(level, cmd);
        self.sync_source(level);
        self.last_action = format!("erase at {}, {}, {}", cell.x, cell.y, cell.z);
    }

    fn sync_source(&mut self, level: &LevelDocument) {
        self.streamer.set_source(level.clone());
    }
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
