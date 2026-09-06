//! Dense voxel chunk storage + dirty tracking.
//!
//! `LevelDocument` today is `HashMap<IVec3, BlockData>` — one heap node
//! per block plus `O(map)` scans on every pulse/theme change. This crate keeps
//! the same logical model but stores each 16^3 chunk densely with a small
//! palette so meshing/physics can iterate arrays instead of hashing, and tracks
//! dirty chunks incrementally (no full-map rescans) with a
//! focus-sorted drain.
//!
//! Pure Rust, no Bevy dependency. Cell/chunk coords are `[i32; 3]` arrays so
//! both the game (`IVec3`) and the worker can convert without pulling Bevy in.

use std::collections::{HashMap, HashSet};

use rustbox_format::BlockData;

pub const CHUNK_SIZE: i32 = 16;
pub const CHUNK_VOLUME: usize = 16 * 16 * 16;
/// Palette sentinel for "empty cell".
const EMPTY: u16 = u16::MAX;

pub type ChunkPos = [i32; 3];
pub type CellPos = [i32; 3];

#[inline]
pub fn chunk_of(cell: CellPos) -> ChunkPos {
    [
        cell[0].div_euclid(CHUNK_SIZE),
        cell[1].div_euclid(CHUNK_SIZE),
        cell[2].div_euclid(CHUNK_SIZE),
    ]
}

#[inline]
pub fn chunk_origin(cpos: ChunkPos) -> CellPos {
    [
        cpos[0] * CHUNK_SIZE,
        cpos[1] * CHUNK_SIZE,
        cpos[2] * CHUNK_SIZE,
    ]
}

#[inline]
pub fn local_of(cell: CellPos) -> [i32; 3] {
    [
        cell[0].rem_euclid(CHUNK_SIZE),
        cell[1].rem_euclid(CHUNK_SIZE),
        cell[2].rem_euclid(CHUNK_SIZE),
    ]
}

#[inline]
pub fn index_of(local: [i32; 3]) -> usize {
    (local[1] as usize * 16 + local[2] as usize) * 16 + local[0] as usize
}

/// Chunks touched by an edit at `cell` (own chunk + face neighbors when the
/// cell sits on a chunk border). Mirrors `maker::chunk::affected_chunks`.
pub fn affected_chunks(cell: CellPos) -> Vec<ChunkPos> {
    let base = chunk_of(cell);
    let local = local_of(cell);
    let mut out = vec![base];
    for axis in 0..3 {
        if local[axis] == 0 {
            let mut n = base;
            n[axis] -= 1;
            out.push(n);
        }
        if local[axis] == CHUNK_SIZE - 1 {
            let mut n = base;
            n[axis] += 1;
            out.push(n);
        }
    }
    out
}

/// One 16^3 chunk stored as palette + indices.
///
/// Most maker levels use a handful of distinct `(kind, shape, rot,
/// waterlogged)` combos per chunk, so `palette.len() <= ~32` and each cell is
/// 2 bytes instead of a full `BlockData` + `HashMap` node (~48+ bytes).
#[derive(Clone, Debug, Default)]
pub struct DenseChunk {
    indices: Vec<u16>,
    palette: Vec<BlockData>,
}

impl DenseChunk {
    pub fn new() -> Self {
        Self {
            indices: vec![EMPTY; CHUNK_VOLUME],
            palette: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.palette.is_empty() || self.indices.iter().all(|&i| i == EMPTY)
    }

    pub fn filled_count(&self) -> usize {
        self.indices.iter().filter(|&&i| i != EMPTY).count()
    }

    fn palette_id(&mut self, block: &BlockData) -> u16 {
        if let Some(pos) = self.palette.iter().position(|b| {
            b.kind == block.kind
                && b.shape == block.shape
                && b.rot == block.rot
                && b.waterlogged == block.waterlogged
        }) {
            return pos as u16;
        }
        if self.palette.len() >= (EMPTY as usize) {
            return 0;
        }
        self.palette.push(BlockData {
            position: block.position,
            kind: block.kind,
            shape: block.shape,
            rot: block.rot,
            waterlogged: block.waterlogged,
        });
        (self.palette.len() - 1) as u16
    }

    pub fn get_local(&self, local: [i32; 3]) -> Option<&BlockData> {
        let id = *self.indices.get(index_of(local))?;
        if id == EMPTY {
            return None;
        }
        self.palette.get(id as usize)
    }

    /// Returns true if the cell changed.
    pub fn set_local(&mut self, local: [i32; 3], block: Option<BlockData>) -> bool {
        let idx = index_of(local);
        match block {
            Some(b) => {
                let id = self.palette_id(&b);
                if self.indices[idx] == id {
                    return false;
                }
                self.indices[idx] = id;
                true
            }
            None => {
                if self.indices[idx] == EMPTY {
                    return false;
                }
                self.indices[idx] = EMPTY;
                true
            }
        }
    }

    /// Drop unreferenced palette entries.
    pub fn compact(&mut self) {
        let mut used = vec![false; self.palette.len()];
        for &id in &self.indices {
            if id != EMPTY {
                used[id as usize] = true;
            }
        }
        if used.iter().all(|&u| u) {
            return;
        }
        let mut remap = vec![EMPTY; self.palette.len()];
        let mut new_palette = Vec::with_capacity(self.palette.len());
        for (old, block) in self.palette.drain(..).enumerate() {
            if used[old] {
                remap[old] = new_palette.len() as u16;
                new_palette.push(block);
            }
        }
        self.palette = new_palette;
        for id in self.indices.iter_mut() {
            if *id != EMPTY {
                *id = remap[*id as usize];
            }
        }
    }

    pub fn iter_filled(&self) -> impl Iterator<Item = ([i32; 3], &BlockData)> + '_ {
        self.indices.iter().enumerate().filter_map(|(i, &id)| {
            if id == EMPTY {
                return None;
            }
            let lx = (i % 16) as i32;
            let lz = ((i / 16) % 16) as i32;
            let ly = (i / 256) as i32;
            Some(([lx, ly, lz], &self.palette[id as usize]))
        })
    }
}

/// Sparse grid of dense chunks + incremental dirty tracking.
///
/// Replaces the `O(map)` patterns in `rendering.rs:970-977`
/// (`occupied`/`water_occupied` full scans) and
/// `level.rs:243-249` (`mark_pulse_dirty` full scan) with:
/// - `occupied` maintained on write (no rescan),
/// - `dirty` extended via [`affected_chunks`],
/// - [`VoxelGrid::drain_sorted`] focus-sorted
///   `terrain/mod.rs:1038-1042 min_by_key(distance, tick)`.
#[derive(Clone, Debug, Default)]
pub struct VoxelGrid {
    chunks: HashMap<ChunkPos, DenseChunk>,
    dirty: HashSet<ChunkPos>,
    /// Ticks since each chunk was queued (`started_tick` fairness).
    queued_tick: HashMap<ChunkPos, u64>,
    tick: u64,
}

impl VoxelGrid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_blocks(blocks: impl IntoIterator<Item = BlockData>) -> Self {
        let mut grid = Self::new();
        for b in blocks {
            grid.set(b.position, Some(b));
        }
        grid.clear_dirty();
        grid
    }

    pub fn get(&self, cell: CellPos) -> Option<&BlockData> {
        let cpos = chunk_of(cell);
        self.chunks.get(&cpos)?.get_local(local_of(cell))
    }

    /// Set/erase a cell. Marks affected chunks dirty. Prunes newly-empty
    /// chunks so [`VoxelGrid::occupied_chunks`] never needs a full scan.
    pub fn set(&mut self, cell: CellPos, block: Option<BlockData>) {
        let cpos = chunk_of(cell);
        let local = local_of(cell);
        match block {
            Some(mut b) => {
                b.position = cell;
                let chunk = self.chunks.entry(cpos).or_insert_with(DenseChunk::new);
                if chunk.set_local(local, Some(b)) {
                    self.mark_dirty(cell);
                }
            }
            None => {
                let (changed, became_empty) = match self.chunks.get_mut(&cpos) {
                    Some(chunk) => {
                        let changed = chunk.set_local(local, None);
                        let empty = chunk.is_empty();
                        (changed, empty)
                    }
                    None => (false, false),
                };
                if changed {
                    self.mark_dirty(cell);
                }
                if became_empty {
                    self.chunks.remove(&cpos);
                }
            }
        }
    }

    fn mark_dirty(&mut self, cell: CellPos) {
        self.tick += 1;
        for cpos in affected_chunks(cell) {
            self.dirty.insert(cpos);
            self.queued_tick.entry(cpos).or_insert(self.tick);
        }
    }

    pub fn mark_all_dirty(&mut self) {
        self.tick += 1;
        for cpos in self.chunks.keys().copied().collect::<Vec<_>>() {
            self.dirty.insert(cpos);
            self.queued_tick.entry(cpos).or_insert(self.tick);
        }
    }

    pub fn dirty_count(&self) -> usize {
        self.dirty.len()
    }

    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
        self.queued_tick.clear();
    }

    /// Drain dirty chunks sorted by distance to `focus` (
    /// focus-sorted `mesh_todos`). `focus` is world-space cell coords.
    pub fn drain_sorted(&mut self, focus: [f32; 3]) -> Vec<ChunkPos> {
        let mut out: Vec<ChunkPos> = self.dirty.drain().collect();
        out.sort_by(|a, b| {
            let da = chunk_dist2(*a, focus);
            let db = chunk_dist2(*b, focus);
            da.partial_cmp(&db)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| self.queued_tick.get(a).cmp(&self.queued_tick.get(b)))
        });
        for cpos in &out {
            self.queued_tick.remove(cpos);
        }
        out
    }

    /// Chunk positions that currently hold blocks — maintained incrementally,
    /// no full-map scan (fixes `rendering.rs:970-977`).
    pub fn occupied_chunks(&self) -> Vec<ChunkPos> {
        self.chunks
            .iter()
            .filter_map(|(pos, chunk)| (!chunk.is_empty()).then_some(*pos))
            .collect()
    }

    pub fn chunk(&self, cpos: ChunkPos) -> Option<&DenseChunk> {
        self.chunks.get(&cpos)
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    pub fn filled_count(&self) -> usize {
        self.chunks.values().map(|c| c.filled_count()).sum()
    }

    /// Iterate all filled cells in a chunk in world coords.
    pub fn iter_chunk_world(
        &self,
        cpos: ChunkPos,
    ) -> impl Iterator<Item = (CellPos, &BlockData)> + '_ {
        let origin = chunk_origin(cpos);
        self.chunks
            .get(&cpos)
            .into_iter()
            .flat_map(move |chunk| chunk.iter_filled())
            .map(move |(local, block)| {
                (
                    [
                        origin[0] + local[0],
                        origin[1] + local[1],
                        origin[2] + local[2],
                    ],
                    block,
                )
            })
    }
}

fn chunk_dist2(cpos: ChunkPos, focus: [f32; 3]) -> f32 {
    let origin = chunk_origin(cpos);
    let cx = origin[0] as f32 + CHUNK_SIZE as f32 * 0.5;
    let cy = origin[1] as f32 + CHUNK_SIZE as f32 * 0.5;
    let cz = origin[2] as f32 + CHUNK_SIZE as f32 * 0.5;
    let dx = cx - focus[0];
    let dy = cy - focus[1];
    let dz = cz - focus[2];
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustbox_format::{BlockKind, BlockShape};

    fn block(x: i32, y: i32, z: i32) -> BlockData {
        BlockData {
            position: [x, y, z],
            kind: BlockKind::Grass,
            shape: BlockShape::Full,
            rot: 0,
            waterlogged: false,
        }
    }

    #[test]
    fn set_get_roundtrip() {
        let mut grid = VoxelGrid::new();
        grid.set([1, 2, 3], Some(block(1, 2, 3)));
        assert!(grid.get([1, 2, 3]).is_some());
        assert!(grid.get([1, 2, 4]).is_none());
    }

    #[test]
    fn erase_prunes_chunk() {
        let mut grid = VoxelGrid::new();
        grid.set([0, 0, 0], Some(block(0, 0, 0)));
        assert_eq!(grid.chunk_count(), 1);
        grid.set([0, 0, 0], None);
        assert_eq!(grid.chunk_count(), 0);
        assert!(grid.occupied_chunks().is_empty());
    }

    #[test]
    fn border_edit_dirties_neighbors() {
        let mut grid = VoxelGrid::new();
        grid.clear_dirty();
        grid.set([16, 0, 0], Some(block(16, 0, 0)));
        assert!(grid.dirty_count() >= 2);
    }

    #[test]
    fn drain_sorted_by_focus() {
        let mut grid = VoxelGrid::new();
        grid.set([8, 8, 8], Some(block(8, 8, 8)));
        grid.set([168, 8, 8], Some(block(168, 8, 8)));
        grid.clear_dirty();
        grid.set([9, 8, 8], Some(block(9, 8, 8)));
        grid.set([169, 8, 8], Some(block(169, 8, 8)));
        let drained = grid.drain_sorted([0.0, 0.0, 0.0]);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0], [0, 0, 0]);
    }

    #[test]
    fn palette_dedups() {
        let mut chunk = DenseChunk::new();
        for x in 0..16 {
            chunk.set_local([x, 0, 0], Some(block(x, 0, 0)));
        }
        assert_eq!(chunk.palette.len(), 1);
        assert_eq!(chunk.filled_count(), 16);
    }
}
