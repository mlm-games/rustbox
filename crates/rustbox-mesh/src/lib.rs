//! Greedy + rotation-aware voxel meshing.
//!
//! Merges coplanar faces instead of emitting 4v/6i per face, classifies faces
//! explicitly (`Opaque`/`Fluid`/`Empty`), bakes per-vertex shade at build
//! time, and stays pure `Send` data so callers can move builds onto a background
//! task pool (main thread only uploads).
//!
//! Scope: greedy merge applies to axis-aligned Full boxes of the same kind
//! (the common maker case: ground planes, walls). Slabs/ramps fall back to
//! exact per-face quads so visuals never change — only the quad count and the
//! occlusion correctness do.

use std::collections::HashMap;

use rustbox_format::{BlockData, BlockKind, BlockShape};
use rustbox_voxel::{CHUNK_SIZE, CellPos, VoxelGrid, chunk_origin};

/// Pure mesh data — convertible to `bevy::mesh::Mesh` by the game crate
/// without this crate depending on Bevy (mesh compute / render split).
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn quad_count(&self) -> usize {
        self.indices.len() / 6
    }

    pub fn push_quad(&mut self, verts: [[f32; 3]; 4], color: [f32; 4]) {
        let n = {
            let a = verts[0];
            let b = verts[1];
            let c = verts[2];
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
            [n[0] / len, n[1] / len, n[2] / len]
        };
        let base = self.positions.len() as u32;
        let uvs = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        for (i, p) in verts.into_iter().enumerate() {
            self.positions.push(p);
            self.normals.push(n);
            self.colors.push(color);
            self.uvs.push(uvs[i]);
        }
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
    }

    /// Merged axis-aligned quad (merged-quad helper).
    /// `origin` is the min corner, `u`/`v` are edge vectors.
    pub fn push_merged_quad(
        &mut self,
        origin: [f32; 3],
        u: [f32; 3],
        v: [f32; 3],
        color: [f32; 4],
    ) {
        self.push_quad(
            [
                origin,
                [origin[0] + u[0], origin[1] + u[1], origin[2] + u[2]],
                [
                    origin[0] + u[0] + v[0],
                    origin[1] + u[1] + v[1],
                    origin[2] + u[2] + v[2],
                ],
                [origin[0] + v[0], origin[1] + v[1], origin[2] + v[2]],
            ],
            color,
        );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaceKind {
    Empty,
    Opaque,
    Fluid,
}

pub fn classify(kind: BlockKind, pulse_on: bool) -> FaceKind {
    match kind {
        BlockKind::Spawn => FaceKind::Empty,
        BlockKind::Water => FaceKind::Fluid,
        BlockKind::TimedPulse if !pulse_on => FaceKind::Empty,
        _ => FaceKind::Opaque,
    }
}

/// Rotate a world-space face dir into a block's shape-local frame.
///
/// Inverse of the renderer's `rotate_y` (which rotates local verts into the
/// world). Fixes `rendering.rs:759` which tested `cell + f.dir` with the
/// *unrotated* dir while drawing rotated verts.
pub fn world_dir_to_local(dir: [i32; 3], rot: u8) -> [i32; 3] {
    match rot % 4 {
        0 => dir,
        1 => [-dir[2], dir[1], dir[0]],
        2 => [-dir[0], dir[1], -dir[2]],
        3 => [dir[2], dir[1], -dir[0]],
        _ => dir,
    }
}

/// Does a neighbor block fully cover the face between us and it?
///
/// Conservative per-interface check: only axis-aligned Full neighbors
/// occlude; slabs occlude only the side they actually fill; slopes/corners
/// never occlude (avoids hole-punching) and
/// `VerticalSlab` only occludes its own slab plane.
pub fn neighbor_occludes(
    neighbor: &BlockData,
    face_dir_world: [i32; 3],
    neighbor_solid: bool,
) -> bool {
    if !neighbor_solid {
        return false;
    }
    let local_dir = world_dir_to_local(face_dir_world, neighbor.rot);
    match neighbor.shape {
        BlockShape::Full => true,
        BlockShape::Half => local_dir == [0, -1, 0],
        BlockShape::TopHalf => local_dir == [0, 1, 0],
        BlockShape::Thin => local_dir == [0, 1, 0],
        BlockShape::VerticalSlab => local_dir == [0, 0, -1],
        _ => false,
    }
}

/// Directional shade baked per face (no sunlight sim yet, but top/side/bottom
/// contrast is now data, not material hacks).
pub fn shade_for_dir(dir: [i32; 3]) -> f32 {
    if dir == [0, 1, 0] {
        1.0
    } else if dir == [0, -1, 0] {
        0.55
    } else if dir[0] != 0 {
        0.8
    } else {
        0.7
    }
}

pub fn shaded_color(base: [f32; 4], dir: [i32; 3]) -> [f32; 4] {
    let s = shade_for_dir(dir);
    [base[0] * s, base[1] * s, base[2] * s, base[3]]
}

pub const DIRS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

#[derive(Clone, Debug)]
pub struct ChunkMeshInput {
    pub pulse_on: bool,
    /// Base color per kind (linear). Shade is multiplied per face.
    pub kind_color: HashMap<BlockKind, [f32; 4]>,
    pub water_color: [f32; 4],
    /// Kind×shape pairs replaced by pack models (skipped in chunk mesh, same
    /// as the sync overlay path).
    pub skip_overlay: std::collections::HashSet<(BlockKind, BlockShape)>,
    /// Global water plane Y (`None` = no plane). Waterlogged or submerged
    /// opaque cells get the submerged tint like the sync path.
    pub water_level: Option<i32>,
    /// Tint for submerged/waterlogged opaque faces.
    pub submerged_tint: [f32; 4],
}

impl Default for ChunkMeshInput {
    fn default() -> Self {
        Self {
            pulse_on: true,
            kind_color: HashMap::new(),
            water_color: [0.2, 0.45, 0.85, 0.72],
            skip_overlay: std::collections::HashSet::new(),
            water_level: None,
            submerged_tint: [0.55, 0.62, 0.78, 1.0],
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChunkMeshOutput {
    pub opaque: HashMap<BlockKind, MeshData>,
    pub water: MeshData,
    /// Greedy-merged quads (subset of opaque quads) — for stats/tests.
    pub merged_quads: usize,
    /// Fallback per-face quads (slopes/corners/edges).
    pub fallback_quads: usize,
}

/// Build one chunk's meshes from a [`VoxelGrid`].
///
/// - Full opaque boxes of the same kind are greedy-merged per axis layer
///   (`greedy.rs`).
/// - Everything else uses exact per-face quads with rotation-aware occlusion.
/// - Neighbor lookups cross chunk borders via the grid (fixes seam holes).
/// - Pure + `Send`: safe to run on `AsyncComputeTaskPool`.
pub fn build_chunk_mesh(
    grid: &VoxelGrid,
    cpos: [i32; 3],
    input: &ChunkMeshInput,
    is_solid: impl Fn(BlockKind) -> bool + Copy,
) -> ChunkMeshOutput {
    let mut out = ChunkMeshOutput::default();
    let origin = chunk_origin(cpos);

    let mut full_cells: HashMap<BlockKind, Vec<[i32; 3]>> = HashMap::new();
    let mut fallback: Vec<(CellPos, BlockData)> = Vec::new();

    for lx in 0..CHUNK_SIZE {
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let cell = [origin[0] + lx, origin[1] + ly, origin[2] + lz];
                let Some(block) = grid.get(cell) else {
                    continue;
                };
                if classify(block.kind, input.pulse_on) == FaceKind::Empty {
                    continue;
                }
                if block.kind == BlockKind::Water {
                    continue; // handled below
                }
                if input.skip_overlay.contains(&(block.kind, block.shape)) {
                    continue;
                }
                // Submerged Fulls go exact so the water tint isn't lost in a
                // merged dry rect.
                if block.shape == BlockShape::Full
                    && block.rot == 0
                    && !is_submerged(cell, block, input)
                {
                    full_cells.entry(block.kind).or_default().push(cell);
                } else {
                    fallback.push((cell, block.clone()));
                }
            }
        }
    }

    for (kind, cells) in full_cells {
        // Filter overlay kinds that slipped in (grid predates manifest check).
        let cells: Vec<[i32; 3]> = cells
            .into_iter()
            .filter(|cell| {
                grid.get(*cell).is_none_or(|b| {
                    !input.skip_overlay.contains(&(b.kind, b.shape))
                })
            })
            .collect();
        if cells.is_empty() {
            continue;
        }
        let mut color = input
            .kind_color
            .get(&kind)
            .copied()
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        let _ = &mut color;
        let mesh = out.opaque.entry(kind).or_default();
        let before = mesh.quad_count();
        greedy_full_faces(grid, &cells, input, is_solid, mesh);
        out.merged_quads += mesh.quad_count() - before;
    }

    for (cell, block) in fallback {
        let mut color = input
            .kind_color
            .get(&block.kind)
            .copied()
            .unwrap_or([1.0, 1.0, 1.0, 1.0]);
        if is_submerged(cell, &block, input) {
            color = input.submerged_tint;
        }
        let mesh = out.opaque.entry(block.kind).or_default();
        let before = mesh.quad_count();
        push_box_faces(grid, cell, &block, input.pulse_on, is_solid, color, mesh);
        out.fallback_quads += mesh.quad_count() - before;
    }

    for lx in 0..CHUNK_SIZE {
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let cell = [origin[0] + lx, origin[1] + ly, origin[2] + lz];
                let Some(block) = grid.get(cell) else {
                    continue;
                };
                if block.kind != BlockKind::Water {
                    continue;
                }
                for dir in DIRS {
                    let ncell = [cell[0] + dir[0], cell[1] + dir[1], cell[2] + dir[2]];
                    let covered = match grid.get(ncell) {
                        None => false,
                        Some(nb) => {
                            nb.kind == BlockKind::Water
                                || (is_solid(nb.kind)
                                    && classify(nb.kind, input.pulse_on) == FaceKind::Opaque)
                        }
                    };
                    if covered {
                        continue;
                    }
                    let f = cell_f32(cell);
                    let quad = full_face_quad(f, dir);
                    out.water.push_quad(quad, input.water_color);
                }
            }
        }
    }

    out.opaque.retain(|_, m| !m.is_empty());
    out
}

fn cell_f32(cell: CellPos) -> [f32; 3] {
    [cell[0] as f32, cell[1] as f32, cell[2] as f32]
}

fn full_face_quad(origin: [f32; 3], dir: [i32; 3]) -> [[f32; 3]; 4] {
    let [x, y, z] = origin;
    match dir {
        [1, 0, 0] => [
            [x + 1.0, y, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y, z + 1.0],
        ],
        [-1, 0, 0] => [
            [x, y, z + 1.0],
            [x, y + 1.0, z + 1.0],
            [x, y + 1.0, z],
            [x, y, z],
        ],
        [0, 1, 0] => [
            [x, y + 1.0, z],
            [x, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x + 1.0, y + 1.0, z],
        ],
        [0, -1, 0] => [
            [x, y, z],
            [x + 1.0, y, z],
            [x + 1.0, y, z + 1.0],
            [x, y, z + 1.0],
        ],
        [0, 0, 1] => [
            [x, y, z + 1.0],
            [x + 1.0, y, z + 1.0],
            [x + 1.0, y + 1.0, z + 1.0],
            [x, y + 1.0, z + 1.0],
        ],
        _ => [
            [x, y, z],
            [x, y + 1.0, z],
            [x + 1.0, y + 1.0, z],
            [x + 1.0, y, z],
        ],
    }
}

/// Greedy-merge Full faces: for each of the 6 dirs, for each slice
/// perpendicular to dir, build a 16x16 occupancy mask of faces that need
/// emitting, then expand maximal rects.
fn is_submerged(cell: CellPos, block: &BlockData, input: &ChunkMeshInput) -> bool {
    if block.waterlogged {
        return true;
    }
    if let Some(wl) = input.water_level {
        return cell[1] < wl;
    }
    false
}

fn greedy_full_faces(
    grid: &VoxelGrid,
    cells: &[[i32; 3]],
    input: &ChunkMeshInput,
    is_solid: impl Fn(BlockKind) -> bool + Copy,
    mesh: &mut MeshData,
) {
    use std::collections::HashSet;
    let set: HashSet<[i32; 3]> = cells.iter().copied().collect();
    // All greedy cells share one kind here; tint once (submerged handled
    // per-merged-quad below via the anchor cell).
    let kind = cells
        .first()
        .and_then(|c| grid.get(*c))
        .map(|b| b.kind);

    for dir in DIRS {
        let mut slices: HashMap<i32, Vec<[i32; 3]>> = HashMap::new();
        for &cell in cells {
            let ncell = [cell[0] + dir[0], cell[1] + dir[1], cell[2] + dir[2]];
            let occluded = match grid.get(ncell) {
                None => false,
                Some(nb) => {
                    classify(nb.kind, input.pulse_on) == FaceKind::Opaque
                        && is_solid(nb.kind)
                        && neighbor_occludes(nb, [-dir[0], -dir[1], -dir[2]], true)
                }
            };
            if occluded {
                continue;
            }
            let slice = match dir {
                [1, 0, 0] | [-1, 0, 0] => cell[0],
                [0, 1, 0] | [0, -1, 0] => cell[1],
                _ => cell[2],
            };
            slices.entry(slice).or_default().push(cell);
        }

        for (_slice, members) in slices {
            let mut mask = [[false; 16]; 16];
            for &cell in &members {
                let (u, v) = mask_coords(cell, dir);
                if (0..16).contains(&u) && (0..16).contains(&v) {
                    mask[v as usize][u as usize] = true;
                }
            }
            let mut visited = [[false; 16]; 16];
            for v in 0..16 {
                for u in 0..16 {
                    if !mask[v][u] || visited[v][u] {
                        continue;
                    }
                    let mut w = 1;
                    while u + w < 16 && mask[v][u + w] && !visited[v][u + w] {
                        w += 1;
                    }
                    let mut h = 1;
                    'outer: while v + h < 16 {
                        for k in 0..w {
                            if !mask[v + h][u + k] || visited[v + h][u + k] {
                                break 'outer;
                            }
                        }
                        h += 1;
                    }
                    for dv in 0..h {
                        for du in 0..w {
                            visited[v + dv][u + du] = true;
                        }
                    }
                    let (origin, eu, ev) = merged_quad_frame(&members, dir, u, v, w, h);
                    let _ = &set;
                    let base = kind
                        .and_then(|k| input.kind_color.get(&k).copied())
                        .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                    // Submerged tint follows the anchor cell of the rect.
                    mesh.push_merged_quad(origin, eu, ev, shaded_color(base, dir));
                }
            }
        }
    }
}

/// Mask coords (u,v) for a cell on a face perpendicular to `dir`.
fn mask_coords(cell: [i32; 3], dir: [i32; 3]) -> (i32, i32) {
    let lx = cell[0].rem_euclid(16);
    let ly = cell[1].rem_euclid(16);
    let lz = cell[2].rem_euclid(16);
    match dir {
        [1, 0, 0] | [-1, 0, 0] => (lz, ly),
        [0, 1, 0] | [0, -1, 0] => (lx, lz),
        _ => (lx, ly),
    }
}

/// World-space frame for a merged rect: origin + edge vectors.
/// Reconstructs from the slice members' bounding origin to stay exact even
/// when the chunk isn't at the origin.
fn merged_quad_frame(
    members: &[[i32; 3]],
    dir: [i32; 3],
    u: usize,
    v: usize,
    w: usize,
    h: usize,
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let any = members[0];
    let ox = any[0].div_euclid(16) * 16;
    let oy = any[1].div_euclid(16) * 16;
    let oz = any[2].div_euclid(16) * 16;
    match dir {
        [1, 0, 0] => {
            let px = members.iter().map(|c| c[0]).max().unwrap_or(ox) + 1;
            (
                [px as f32, oy as f32 + v as f32, oz as f32 + u as f32],
                [0.0, 0.0, w as f32],
                [0.0, h as f32, 0.0],
            )
        }
        [-1, 0, 0] => {
            let px = members.iter().map(|c| c[0]).min().unwrap_or(ox);
            (
                [px as f32, oy as f32 + v as f32, oz as f32 + u as f32],
                [0.0, 0.0, w as f32],
                [0.0, h as f32, 0.0],
            )
        }
        [0, 1, 0] => {
            let py = members.iter().map(|c| c[1]).max().unwrap_or(oy) + 1;
            (
                [ox as f32 + u as f32, py as f32, oz as f32 + v as f32],
                [w as f32, 0.0, 0.0],
                [0.0, 0.0, h as f32],
            )
        }
        [0, -1, 0] => {
            let py = members.iter().map(|c| c[1]).min().unwrap_or(oy);
            (
                [ox as f32 + u as f32, py as f32, oz as f32 + v as f32],
                [w as f32, 0.0, 0.0],
                [0.0, 0.0, h as f32],
            )
        }
        [0, 0, 1] => {
            let pz = members.iter().map(|c| c[2]).max().unwrap_or(oz) + 1;
            (
                [ox as f32 + u as f32, oy as f32 + v as f32, pz as f32],
                [w as f32, 0.0, 0.0],
                [0.0, h as f32, 0.0],
            )
        }
        _ => {
            let pz = members.iter().map(|c| c[2]).min().unwrap_or(oz);
            (
                [ox as f32 + u as f32, oy as f32 + v as f32, pz as f32],
                [w as f32, 0.0, 0.0],
                [0.0, h as f32, 0.0],
            )
        }
    }
}

/// Exact per-face quads for shaped (non-Full) boxes: Half/TopHalf/Thin as
/// true boxes; other shapes as full-cube faces with rotation-aware culling
/// (conservative — never punches holes — caller replaces with exact art for
/// slopes via the existing `shape_faces` path until migrated).
fn push_box_faces(
    grid: &VoxelGrid,
    cell: CellPos,
    block: &BlockData,
    pulse_on: bool,
    is_solid: impl Fn(BlockKind) -> bool + Copy,
    color: [f32; 4],
    mesh: &mut MeshData,
) {
    let (y0, y1): (f32, f32) = match block.shape {
        BlockShape::Half => (0.0, 0.5),
        BlockShape::TopHalf => (0.5, 1.0),
        BlockShape::Thin => (1.0 - rustbox_format::block::THIN_HEIGHT, 1.0),
        _ => (0.0, 1.0),
    };
    let f = cell_f32(cell);
    for dir in DIRS {
        let ncell = [cell[0] + dir[0], cell[1] + dir[1], cell[2] + dir[2]];
        let occluded = match grid.get(ncell) {
            None => false,
            Some(nb) => {
                classify(nb.kind, pulse_on) == FaceKind::Opaque
                    && is_solid(nb.kind)
                    && neighbor_occludes(nb, [-dir[0], -dir[1], -dir[2]], true)
                    && our_side_full(block.shape, block.rot, dir)
            }
        };
        if occluded {
            continue;
        }
        let quad = shaped_face_quad(f, dir, y0, y1);
        mesh.push_quad(quad, shaded_color(color, dir));
    }
}

fn our_side_full(shape: BlockShape, _rot: u8, dir: [i32; 3]) -> bool {
    match shape {
        BlockShape::Full => true,
        BlockShape::Half => dir != [0, 1, 0],
        BlockShape::TopHalf => dir != [0, -1, 0],
        BlockShape::Thin => dir != [0, -1, 0],
        BlockShape::VerticalSlab => true,
        _ => dir == [0, -1, 0],
    }
}

fn shaped_face_quad(f: [f32; 3], dir: [i32; 3], y0: f32, y1: f32) -> [[f32; 3]; 4] {
    let [x, y, z] = f;
    match dir {
        [1, 0, 0] => [
            [x + 1.0, y + y0, z],
            [x + 1.0, y + y1, z],
            [x + 1.0, y + y1, z + 1.0],
            [x + 1.0, y + y0, z + 1.0],
        ],
        [-1, 0, 0] => [
            [x, y + y0, z + 1.0],
            [x, y + y1, z + 1.0],
            [x, y + y1, z],
            [x, y + y0, z],
        ],
        [0, 1, 0] => [
            [x, y + y1, z],
            [x, y + y1, z + 1.0],
            [x + 1.0, y + y1, z + 1.0],
            [x + 1.0, y + y1, z],
        ],
        [0, -1, 0] => [
            [x, y + y0, z],
            [x + 1.0, y + y0, z],
            [x + 1.0, y + y0, z + 1.0],
            [x, y + y0, z + 1.0],
        ],
        [0, 0, 1] => [
            [x, y + y0, z + 1.0],
            [x + 1.0, y + y0, z + 1.0],
            [x + 1.0, y + y1, z + 1.0],
            [x, y + y1, z + 1.0],
        ],
        _ => [
            [x, y + y0, z],
            [x, y + y1, z],
            [x + 1.0, y + y1, z],
            [x + 1.0, y + y0, z],
        ],
    }
}

/// Bevy-free helper for callers that still use the legacy `shape_faces` path:
/// rotate a world offset (cell-relative [0,1]) into shape-local coords.
pub fn local_from_world_01(wx: f32, wz: f32, rot: u8) -> (f32, f32) {
    use std::f32::consts::FRAC_PI_2;
    let angle = rot as f32 * FRAC_PI_2;
    let (s, c) = angle.sin_cos();
    let sx = wx - 0.5;
    let sz = wz - 0.5;
    (c * sx - s * sz + 0.5, s * sx + c * sz + 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustbox_format::{BlockKind, BlockShape};
    use rustbox_voxel::VoxelGrid;

    fn full(x: i32, y: i32, z: i32, kind: BlockKind) -> BlockData {
        BlockData {
            position: [x, y, z],
            kind,
            shape: BlockShape::Full,
            rot: 0,
            waterlogged: false,
        }
    }

    fn is_solid(kind: BlockKind) -> bool {
        kind.is_solid()
    }

    #[test]
    fn greedy_merges_flat_ground() {
        let mut grid = VoxelGrid::new();
        for x in 0..8 {
            for z in 0..8 {
                grid.set([x, 0, z], Some(full(x, 0, z, BlockKind::Grass)));
            }
        }
        let input = ChunkMeshInput::default();
        let output = build_chunk_mesh(&grid, [0, 0, 0], &input, is_solid);
        let mesh = &output.opaque[&BlockKind::Grass];
        assert!(
            mesh.quad_count() <= 12,
            "expected greedy merge, got {} quads",
            mesh.quad_count()
        );
        assert!(output.merged_quads > 0);
    }

    #[test]
    fn interior_faces_culled() {
        let mut grid = VoxelGrid::new();
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    grid.set([x, y, z], Some(full(x, y, z, BlockKind::Stone)));
                }
            }
        }
        let output = build_chunk_mesh(&grid, [0, 0, 0], &ChunkMeshInput::default(), is_solid);
        assert_eq!(output.opaque[&BlockKind::Stone].quad_count(), 6);
    }

    #[test]
    fn rotation_aware_occlusion() {
        let slab = BlockData {
            position: [1, 0, 0],
            kind: BlockKind::Stone,
            shape: BlockShape::VerticalSlab,
            rot: 0,
            waterlogged: false,
        };
        assert!(neighbor_occludes(&slab, [0, 0, -1], true));
        assert!(!neighbor_occludes(&slab, [0, 0, 1], true));
        let slab180 = BlockData {
            rot: 2,
            ..slab.clone()
        };
        assert!(!neighbor_occludes(&slab180, [0, 0, -1], true));
        assert!(neighbor_occludes(&slab180, [0, 0, 1], true));
        let slope = BlockData {
            shape: BlockShape::Slope,
            rot: 0,
            ..slab
        };
        assert!(!neighbor_occludes(&slope, [1, 0, 0], true));
    }

    #[test]
    fn pulse_off_culls() {
        assert_eq!(classify(BlockKind::TimedPulse, false), FaceKind::Empty);
        assert_eq!(classify(BlockKind::TimedPulse, true), FaceKind::Opaque);
    }
}
