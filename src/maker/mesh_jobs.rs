//! Background mesh jobs: snapshot → task pool → upload.
//!
//! The old path builds every dirty chunk inline in `Update`
//! (`rebuild_dirty_chunks`), so a large paste hitches the frame. This module
//! keeps the same output but splits the work:
//!
//! 1. `dispatch_mesh_jobs` snapshots the needed blocks into an owned
//!    `VoxelGrid` + submits pure `rustbox-mesh` builds to
//!    `AsyncComputeTaskPool`.
//! 2. Worker tasks build `MeshData` off the main thread (pure, `Send`).
//! 3. `poll_mesh_jobs` receives finished `MeshData` and only does the cheap
//!    main-thread part: `meshes.add()` + `spawn()`.
//!
//! Enabled via `UseAsyncMesh(true)`. Off by default until the cutover is
//! validated level-by-level; when off the systems are no-ops and the existing
//! sync path runs unchanged.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use crossbeam_channel::{Receiver, Sender, unbounded};

use rustbox_format::BlockKind;
use rustbox_mesh::{ChunkMeshInput, ChunkMeshOutput};
use rustbox_voxel::VoxelGrid;

use super::MakerCleanup;
use super::block::BlockKindColor;
use super::level::LevelDocument;
use super::rendering::MakerAssets;
use crate::maker::chunk::chunk_of;

#[derive(Resource, Default)]
pub struct UseAsyncMesh(pub bool);

#[derive(Resource)]
pub struct MeshJobChannels {
    pub tx: Sender<FinishedChunk>,
    pub rx: Receiver<FinishedChunk>,
}

impl Default for MeshJobChannels {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        Self { tx, rx }
    }
}

pub struct FinishedChunk {
    pub cpos: IVec3,
    pub output: ChunkMeshOutput,
    pub kind_colors: HashMap<BlockKind, [f32; 4]>,
    pub water_color: [f32; 4],
}

#[derive(Component)]
pub struct MeshTask(Option<Task<FinishedChunk>>);

/// Snapshot dirty chunks (nearest-first) and submit background builds.
/// Caps submissions per frame so a `mark_all_dirty` doesn't spawn hundreds
/// of tasks at once.
pub fn dispatch_mesh_jobs(
    mut commands: Commands,
    mut level: ResMut<LevelDocument>,
    assets: Option<Res<MakerAssets>>,
    channels: Res<MeshJobChannels>,
    flag: Res<UseAsyncMesh>,
    camera_q: Query<&Transform, With<Camera>>,
) {
    if !flag.0 || assets.is_none() || level.dirty_chunks.is_empty() {
        return;
    }
    let focus = camera_q
        .iter()
        .next()
        .map(|t| t.translation)
        .unwrap_or(Vec3::ZERO);
    let mut dirty = level.drain_dirty_sorted(focus);
    if dirty.is_empty() {
        return;
    }
    const MAX_DISPATCH_PER_FRAME: usize = 8;
    if dirty.len() > MAX_DISPATCH_PER_FRAME {
        let rest: Vec<_> = dirty.split_off(MAX_DISPATCH_PER_FRAME);
        level.dirty_chunks.extend(rest);
    }
    let grid = level.to_voxel_grid();
    let pulse_on = level.pulse_on;
    let theme = level.data.theme;
    let kind_colors = kind_color_table();
    let water_color = super::theme::theme_env(theme)
        .water
        .to_linear()
        .to_f32_array();
    let water_color = [water_color[0], water_color[1], water_color[2], 0.72];

    let pool = AsyncComputeTaskPool::get();
    let grid_shared = std::sync::Arc::new(grid);
    for cpos in dirty {
        let grid_clone = grid_shared.clone();
        let tx = channels.tx.clone();
        let colors = kind_colors.clone();
        let cpos_arr = [cpos.x, cpos.y, cpos.z];
        let task = pool.spawn(async move {
            let input = ChunkMeshInput {
                pulse_on,
                kind_color: colors.clone(),
                water_color,
            };
            let output =
                rustbox_mesh::build_chunk_mesh(&grid_clone, cpos_arr, &input, |k| k.is_solid());
            FinishedChunk {
                cpos,
                output,
                kind_colors: colors,
                water_color,
            }
        });
        commands.spawn(MeshTask(Some(task)));
        let _ = tx;
    }
}

/// Collect finished tasks and forward to the upload channel.
pub fn collect_mesh_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut MeshTask)>,
    channels: Res<MeshJobChannels>,
) {
    for (e, mut task) in tasks.iter_mut() {
        let ready = task.0.as_ref().is_some_and(|t| t.is_finished());
        if ready {
            let taken = task.0.take().map(bevy::tasks::block_on);
            if let Some(finished) = taken {
                let _ = channels.tx.send(finished);
                commands.entity(e).despawn();
            }
        }
    }
}

fn kind_color_table() -> HashMap<BlockKind, [f32; 4]> {
    let mut out = HashMap::new();
    for kind in rustbox_format::ALL_BLOCK_KINDS.iter().copied() {
        let c = kind.color().to_linear().to_f32_array();
        out.insert(kind, c);
    }
    out
}

/// Upload finished `MeshData` to Bevy meshes. Only does `meshes.add()` +
/// `spawn()` on the main thread.
pub fn poll_mesh_jobs(
    mut commands: Commands,
    channels: Res<MeshJobChannels>,
    mut meshes: ResMut<Assets<Mesh>>,
    assets: Option<Res<MakerAssets>>,
    mut chunks: ResMut<super::rendering::ChunkEntities>,
    mut water_chunks: ResMut<super::rendering::WaterChunkEntities>,
) {
    let Some(assets) = assets else { return };
    while let Ok(finished) = channels.rx.try_recv() {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, PrimitiveTopology};
        if let Some(ents) = chunks.0.remove(&finished.cpos) {
            for e in ents {
                commands.entity(e).despawn();
            }
        }
        let mut spawned = Vec::new();
        for (kind, data) in finished.output.opaque {
            if data.is_empty() {
                continue;
            }
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs.clone());
            mesh.insert_indices(Indices::U32(data.indices.clone()));
            let mat = assets
                .kind_mats
                .get(&kind)
                .cloned()
                .unwrap_or_else(|| assets.chunk_material.clone());
            let e = commands
                .spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(mat),
                    Transform::IDENTITY,
                    MakerCleanup,
                ))
                .id();
            spawned.push(e);
        }
        if !spawned.is_empty() {
            chunks.0.insert(finished.cpos, spawned);
        }
        if !finished.output.water.is_empty() {
            let data = &finished.output.water;
            let mut mesh = Mesh::new(
                PrimitiveTopology::TriangleList,
                RenderAssetUsages::default(),
            );
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, data.positions.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors.clone());
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs.clone());
            mesh.insert_indices(Indices::U32(data.indices.clone()));
            let handle = meshes.add(mesh);
            match water_chunks.0.get(&finished.cpos) {
                Some(&e) => {
                    commands.entity(e).insert(Mesh3d(handle));
                }
                None => {
                    let e = commands
                        .spawn((
                            Mesh3d(handle),
                            MeshMaterial3d(assets.water_material.clone()),
                            Transform::IDENTITY,
                            MakerCleanup,
                        ))
                        .id();
                    water_chunks.0.insert(finished.cpos, e);
                }
            }
        } else if let Some(e) = water_chunks.0.remove(&finished.cpos) {
            commands.entity(e).despawn();
        }
    }
}

/// Single-pass occupied sets (replaces the old double full-map scan).
pub fn occupied_sets_fast(
    level: &LevelDocument,
) -> (
    std::collections::HashSet<IVec3>,
    std::collections::HashSet<IVec3>,
) {
    level.occupied_sets()
}

pub fn chunk_of_iv(pos: IVec3) -> IVec3 {
    chunk_of(pos)
}
