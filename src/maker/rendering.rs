use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::world_serialization::WorldAsset;

use rustbox_format::{ALL_BLOCK_KINDS, ALL_BLOCK_SHAPES, BlockShape};

use super::MakerCleanup;
use super::block::{BlockKind, BlockKindColor};
use super::block_asset_manifest::{BlockAssetManifest, BlockTintMode};
use super::chunk::CHUNK_SIZE;
use super::entities_runtime::ModelMaterial;
use super::level::{BlockData, LevelDocument, Theme};
use super::player;
use super::theme;

#[derive(Resource)]
pub struct MakerAssets {
    pub chunk_material: Handle<StandardMaterial>,
    pub water_material: Handle<StandardMaterial>,
    pub player_scene: Handle<WorldAsset>,
    pub player_material: Handle<StandardMaterial>,
    pub preview_mat: Handle<StandardMaterial>,
    pub ghost_mats: HashMap<BlockKind, Handle<StandardMaterial>>,
    /// Soft alpha-blended kind-color materials for placement juice / overlays.
    pub ghost_alpha_mats: HashMap<BlockKind, Handle<StandardMaterial>>,
    /// Fallback only — never force-tints pack albedo for `TintMode::Model`.
    pub model_inert_mat: Handle<StandardMaterial>,
    /// Rot-0 mesh for each block shape (previews/ghosts rotate their Transform).
    pub shape_meshes: HashMap<BlockShape, Handle<Mesh>>,
    /// Real pack models that replace the procedural cube for block kinds with
    /// a `BlockAssetManifest` model (Goal, Bounce, Hazard, Spikes by default).
    pub block_overlays: HashMap<(BlockKind, BlockShape), Handle<WorldAsset>>,
    /// Block kind×shape visual manifest (model/preview/placement) loaded from
    /// `assets/models/blocks.ron`. Overlay decisions read from here so a pack
    /// swap is data-only.
    pub block_manifest: BlockAssetManifest,
}

/// glTF model path for a block kind×shape pair, via the block manifest.
/// `None` = rendered by the procedural chunk mesh. Water always stays
/// procedural (translucent surface), regardless of the manifest.
fn overlay_model<'a>(
    manifest: &'a BlockAssetManifest,
    kind: BlockKind,
    shape: BlockShape,
) -> Option<&'a str> {
    if kind == BlockKind::Water {
        return None;
    }
    manifest.entry(kind, shape).and_then(|e| e.model.as_deref())
}

// Per-pair overlay placement ONLY when a real model exists. Returns
// `(scale, y-offset, tint)` so a spawned overlay can keep the pack albedo
// (`TintMode::Model`) instead of force-tinting flat.
fn overlay_placement(
    manifest: &BlockAssetManifest,
    kind: BlockKind,
    shape: BlockShape,
) -> Option<(f32, f32, BlockTintMode)> {
    let e = manifest.entry(kind, shape)?;
    e.model.as_ref()?; // require a real model
    Some((e.scale, e.y_offset, e.tint))
}

/// Spawned overlay scene entity per overlaid block cell. Keyed like chunks.
#[derive(Resource, Default)]
pub struct BlockOverlayEntities(pub HashMap<IVec3, Entity>);

/// Identity of a spawned block overlay so we can rebuild when kind/shape/rot
/// changes (kind/shape swap must not leave a stale model in a cell).
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct BlockOverlayMeta {
    pub kind: BlockKind,
    pub shape: BlockShape,
    pub rot: u8,
}

#[derive(Resource, Default)]
pub struct ChunkEntities(pub HashMap<IVec3, Entity>);

/// Translucent meshes for placed Water blocks (kept separate from the solid
/// chunk meshes so they can use a blended material).
#[derive(Resource, Default)]
pub struct WaterChunkEntities(pub HashMap<IVec3, Entity>);

#[derive(Component)]
pub struct PlacementPreview;

#[derive(Component)]
pub struct GhostTimer(pub f32);

#[derive(Component)]
pub struct WaterSurface;

#[derive(Component)]
pub struct BoundaryWall;

/// Tracks what the water plane / boundary walls were built for, so they only
/// rebuild when their inputs actually change.
#[derive(Resource)]
pub struct WaterBoundaryState {
    pub water_level: Option<i32>,
    pub size: [i32; 3],
    pub walls: bool,
    pub floor: bool,
    pub ceiling: bool,
    pub theme: Theme,
    pub edit: bool,
}

impl Default for WaterBoundaryState {
    fn default() -> Self {
        Self {
            water_level: None,
            size: [0, 0, 0],
            walls: true,
            floor: false,
            ceiling: false,
            theme: Theme::Grass,
            edit: true,
        }
    }
}

/// 90-degree yaw steps applied to a local vertex around the cell center axis.
fn rotate_y(v: Vec3, rot: u8) -> Vec3 {
    if rot == 0 {
        return v;
    }
    Quat::from_rotation_y(rot as f32 * std::f32::consts::FRAC_PI_2) * v
}

/// A face to emit: `dir` is the neighbor direction to cull against (ZERO =
/// never culled), `verts` are local-space corners.
struct FaceSpec {
    dir: IVec3,
    verts: [Vec3; 4],
}

fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> FaceSpec {
    FaceSpec {
        dir: IVec3::ZERO,
        verts: [a, b, c, d],
    }
}

/// Local-space faces (rot 0) for each shape. Full/Half are boxes, slopes are
/// triangular prisms, corner is a quarter pyramid.
fn shape_faces(shape: BlockShape) -> Vec<FaceSpec> {
    let mut faces = Vec::new();
    match shape {
        BlockShape::Full => {
            for (dir, off, t1, t2) in [
                (IVec3::X, Vec3::new(1., 0., 0.), Vec3::Y, Vec3::Z),
                (IVec3::NEG_X, Vec3::new(0., 0., 0.), Vec3::Z, Vec3::Y),
                (IVec3::Y, Vec3::new(0., 1., 0.), Vec3::Z, Vec3::X),
                (IVec3::NEG_Y, Vec3::new(0., 0., 0.), Vec3::X, Vec3::Z),
                (IVec3::Z, Vec3::new(0., 0., 1.), Vec3::X, Vec3::Y),
                (IVec3::NEG_Z, Vec3::new(0., 0., 0.), Vec3::Y, Vec3::X),
            ] {
                faces.push(FaceSpec {
                    dir,
                    verts: [off, off + t1, off + t1 + t2, off + t2],
                });
            }
        }
        BlockShape::Half => {
            // Bottom half slab: y in [0, 0.5]. Side walls stop at the slab top
            // so they don't overhang the walking surface.
            faces.push(FaceSpec {
                dir: IVec3::X,
                verts: [
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0.5, 0.),
                    Vec3::new(1., 0.5, 1.),
                    Vec3::new(1., 0., 1.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_X,
                verts: [
                    Vec3::new(0., 0., 1.),
                    Vec3::new(0., 0.5, 1.),
                    Vec3::new(0., 0.5, 0.),
                    Vec3::new(0., 0., 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::Y,
                verts: [
                    Vec3::new(0., 0.5, 0.),
                    Vec3::new(0., 0.5, 1.),
                    Vec3::new(1., 0.5, 1.),
                    Vec3::new(1., 0.5, 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::Z,
                verts: [
                    Vec3::new(0., 0., 1.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(1., 0.5, 1.),
                    Vec3::new(0., 0.5, 1.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 0.5, 0.),
                    Vec3::new(1., 0.5, 0.),
                    Vec3::new(1., 0., 0.),
                ],
            });
        }
        BlockShape::TopHalf => {
            // Upper half slab: y in [0.5, 1], hanging from the cell ceiling.
            faces.push(FaceSpec {
                dir: IVec3::X,
                verts: [
                    Vec3::new(1., 0.5, 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 1., 1.),
                    Vec3::new(1., 0.5, 1.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_X,
                verts: [
                    Vec3::new(0., 0.5, 1.),
                    Vec3::new(0., 1., 1.),
                    Vec3::new(0., 1., 0.),
                    Vec3::new(0., 0.5, 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::Y,
                verts: [
                    Vec3::new(0., 1., 0.),
                    Vec3::new(0., 1., 1.),
                    Vec3::new(1., 1., 1.),
                    Vec3::new(1., 1., 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0.5, 0.),
                    Vec3::new(1., 0.5, 0.),
                    Vec3::new(1., 0.5, 1.),
                    Vec3::new(0., 0.5, 1.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::Z,
                verts: [
                    Vec3::new(0., 0.5, 1.),
                    Vec3::new(1., 0.5, 1.),
                    Vec3::new(1., 1., 1.),
                    Vec3::new(0., 1., 1.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 0.5, 0.),
                    Vec3::new(0., 1., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 0.5, 0.),
                ],
            });
        }
        BlockShape::Slope => {
            // Rises toward +X.
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
            // Tall end wall (+X).
            faces.push(FaceSpec {
                dir: IVec3::X,
                verts: [
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 1., 1.),
                    Vec3::new(1., 0., 1.),
                ],
            });
            // +Z side.
            faces.push(FaceSpec {
                dir: IVec3::Z,
                verts: [
                    Vec3::new(0., 0., 1.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(1., 1., 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
            // -Z side.
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(0., 0., 0.),
                ],
            });
            // Ramp surface (never culled).
            faces.push(quad(
                Vec3::new(0., 0., 0.),
                Vec3::new(1., 1., 0.),
                Vec3::new(1., 1., 1.),
                Vec3::new(0., 0., 1.),
            ));
        }
        BlockShape::DSlope => {
            // Rises toward -X.
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
            // Tall end wall (-X).
            faces.push(FaceSpec {
                dir: IVec3::NEG_X,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 1., 0.),
                    Vec3::new(0., 1., 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
            // +Z side.
            faces.push(FaceSpec {
                dir: IVec3::Z,
                verts: [
                    Vec3::new(0., 1., 1.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0., 0., 1.),
                    Vec3::new(0., 1., 1.),
                ],
            });
            // -Z side.
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 1., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 1., 0.),
                ],
            });
            // Ramp surface (never culled).
            faces.push(quad(
                Vec3::new(0., 1., 0.),
                Vec3::new(0., 1., 1.),
                Vec3::new(1., 0., 1.),
                Vec3::new(1., 0., 0.),
            ));
        }
        BlockShape::Corner => {
            // Rises toward +X,+Z (height = (lx+lz)/2).
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
            // -X wall (triangle).
            faces.push(FaceSpec {
                dir: IVec3::NEG_X,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 0., 1.),
                    Vec3::new(0., 0.5, 1.),
                    Vec3::new(0., 0., 0.),
                ],
            });
            // +X wall.
            faces.push(FaceSpec {
                dir: IVec3::X,
                verts: [
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0.5, 0.),
                    Vec3::new(1., 1., 1.),
                    Vec3::new(1., 0., 1.),
                ],
            });
            // -Z wall (triangle).
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 0.5, 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 0., 0.),
                ],
            });
            // +Z wall.
            faces.push(FaceSpec {
                dir: IVec3::Z,
                verts: [
                    Vec3::new(0., 0., 1.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(1., 1., 1.),
                    Vec3::new(0., 0.5, 1.),
                ],
            });
            // Ramp surface (never culled).
            faces.push(quad(
                Vec3::new(0., 0., 0.),
                Vec3::new(1., 0.5, 0.),
                Vec3::new(1., 1., 1.),
                Vec3::new(0., 0.5, 1.),
            ));
        }
        BlockShape::OuterCorner => {
            // Full height at the -X,-Z corner, sloping down to zero toward the
            // +X,+Z corner (a peak).
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
            // Ramp surface (never culled).
            faces.push(quad(
                Vec3::new(0., 1., 0.),
                Vec3::new(0., 0.5, 1.),
                Vec3::new(1., 0., 1.),
                Vec3::new(1., 0.5, 0.),
            ));
            // -X wall (trapezoid).
            faces.push(FaceSpec {
                dir: IVec3::NEG_X,
                verts: [
                    Vec3::new(0., 1., 0.),
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 0., 1.),
                    Vec3::new(0., 0.5, 1.),
                ],
            });
            // -Z wall (trapezoid).
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 1., 0.),
                    Vec3::new(1., 0.5, 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(0., 0., 0.),
                ],
            });
            // +X wall (triangle).
            faces.push(FaceSpec {
                dir: IVec3::X,
                verts: [
                    Vec3::new(1., 0., 1.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0.5, 0.),
                    Vec3::new(1., 0., 1.),
                ],
            });
            // +Z wall (triangle).
            faces.push(FaceSpec {
                dir: IVec3::Z,
                verts: [
                    Vec3::new(0., 0., 1.),
                    Vec3::new(1., 0., 1.),
                    Vec3::new(0., 0.5, 1.),
                    Vec3::new(0., 0., 1.),
                ],
            });
        }
        BlockShape::VerticalSlope => {
            // Full-height quarter against the -X/-Z corner, cut away along the
            // diagonal plane x + z = 1.
            faces.push(FaceSpec {
                dir: IVec3::Y,
                verts: [
                    Vec3::new(0., 1., 0.),
                    Vec3::new(0., 1., 1.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(0., 1., 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(0., 0., 1.),
                    Vec3::new(0., 0., 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_X,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 0., 1.),
                    Vec3::new(0., 1., 1.),
                    Vec3::new(0., 1., 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 1., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 0., 0.),
                ],
            });
            // Diagonal cut (never culled).
            faces.push(quad(
                Vec3::new(1., 0., 0.),
                Vec3::new(1., 1., 0.),
                Vec3::new(0., 1., 1.),
                Vec3::new(0., 0., 1.),
            ));
        }
        BlockShape::VerticalSlab => {
            // 1x1x0.5 slab against the local -Z face (z in [0, 0.5]).
            faces.push(FaceSpec {
                dir: IVec3::X,
                verts: [
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 1., 0.5),
                    Vec3::new(1., 0., 0.5),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_X,
                verts: [
                    Vec3::new(0., 0., 0.5),
                    Vec3::new(0., 1., 0.5),
                    Vec3::new(0., 1., 0.),
                    Vec3::new(0., 0., 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::Y,
                verts: [
                    Vec3::new(0., 1., 0.),
                    Vec3::new(0., 1., 0.5),
                    Vec3::new(1., 1., 0.5),
                    Vec3::new(1., 1., 0.),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Y,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(1., 0., 0.),
                    Vec3::new(1., 0., 0.5),
                    Vec3::new(0., 0., 0.5),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::Z,
                verts: [
                    Vec3::new(0., 0., 0.5),
                    Vec3::new(1., 0., 0.5),
                    Vec3::new(1., 1., 0.5),
                    Vec3::new(0., 1., 0.5),
                ],
            });
            faces.push(FaceSpec {
                dir: IVec3::NEG_Z,
                verts: [
                    Vec3::new(0., 0., 0.),
                    Vec3::new(0., 1., 0.),
                    Vec3::new(1., 1., 0.),
                    Vec3::new(1., 0., 0.),
                ],
            });
        }
        BlockShape::Thin => {
            // Thin top slab: full footprint, y in [1-THIN, 1].
            let h = 1.0 - rustbox_format::block::THIN_HEIGHT;
            for (dir, corners) in [
                (
                    IVec3::X,
                    [[1., h, 0.], [1., 1., 0.], [1., 1., 1.], [1., h, 1.]],
                ),
                (
                    IVec3::NEG_X,
                    [[0., h, 1.], [0., 1., 1.], [0., 1., 0.], [0., h, 0.]],
                ),
                (
                    IVec3::Y,
                    [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]],
                ),
                (
                    IVec3::NEG_Y,
                    [[0., h, 0.], [0., h, 1.], [1., h, 1.], [1., h, 0.]],
                ),
                (
                    IVec3::Z,
                    [[0., h, 1.], [1., h, 1.], [1., 1., 1.], [0., 1., 1.]],
                ),
                (
                    IVec3::NEG_Z,
                    [[0., h, 0.], [0., 1., 0.], [1., 1., 0.], [1., h, 0.]],
                ),
            ] {
                faces.push(FaceSpec {
                    dir,
                    verts: corners.map(Vec3::from_array),
                });
            }
        }
    }
    for f in &mut faces {
        for v in f.verts.iter_mut() {
            *v -= Vec3::splat(0.5);
        }
    }
    faces
}

/// Whether a solid `neighbor` fully hides one of our faces in `dir`.
fn face_occluded(level: &LevelDocument, shape: BlockShape, neighbor: Option<&BlockData>) -> bool {
    let Some(nb) = neighbor else {
        return false;
    };
    if !level.kind_is_solid(nb.kind) {
        return false;
    }
    // A box-shaped neighbor with a full footprint covers an axis-aligned box
    // face. Sloped/cut shapes keep their side walls (culling them would punch
    // holes), so only boxes cull boxes.
    let boxy = matches!(
        nb.shape,
        BlockShape::Full | BlockShape::Half | BlockShape::TopHalf | BlockShape::Thin
    );
    match shape {
        BlockShape::Full | BlockShape::Half | BlockShape::TopHalf | BlockShape::Thin => boxy,
        BlockShape::VerticalSlab => false,
        _ => true,
    }
}

/// Is a solid/water neighbor fully covering one of our faces?
fn face_covered_by_neighbor(level: &LevelDocument, nb: &BlockData) -> bool {
    nb.kind == BlockKind::Water || level.kind_is_solid(nb.kind)
}

struct MeshOut {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

fn push_quad(out: &mut MeshOut, v: [Vec3; 4], color: [f32; 4]) {
    let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize();
    let base = out.positions.len() as u32;
    for p in v {
        out.positions.push(p.to_array());
        out.normals.push(n.to_array());
        out.colors.push(color);
    }
    out.indices
        .extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 3, base]);
}

fn build_shape_mesh(shape: BlockShape) -> Mesh {
    let mut out = MeshOut {
        positions: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
    };
    let color = [1.0, 1.0, 1.0, 1.0];
    for f in shape_faces(shape) {
        push_quad(&mut out, f.verts, color);
    }
    finish_mesh(out)
}

fn finish_mesh(out: MeshOut) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, out.positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, out.normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, out.colors);
    mesh.insert_indices(Indices::U32(out.indices));
    mesh
}

/// Append the geometry for a single block at `cell` into `out`.
fn append_block(
    out: &mut MeshOut,
    level: &LevelDocument,
    cell: IVec3,
    block: &BlockData,
    manifest: &BlockAssetManifest,
) {
    if block.kind == BlockKind::Water {
        return;
    }
    // Landmark kinds with a real glTF model keep the cell empty in the chunk
    // mesh; the model scene (spawned by reconcile_block_overlays) replaces it.
    if overlay_model(manifest, block.kind, block.shape).is_some() {
        return;
    }
    // Timed Pulse blocks disappear while the pulse is off.
    if block.kind.is_pulse() && !level.pulse_on {
        return;
    }
    let color = block.kind.color().to_linear().to_f32_array();
    let color = if level.cell_water(cell) {
        [color[0] * 0.55, color[1] * 0.62, color[2] * 0.78, color[3]]
    } else {
        color
    };
    let origin = cell.as_vec3();
    for f in shape_faces(block.shape) {
        if f.dir != IVec3::ZERO {
            let neighbor = level.get_block(cell + f.dir);
            if face_occluded(level, block.shape, neighbor) {
                continue;
            }
        }
        let verts = f
            .verts
            .map(|p| origin + rotate_y(p, block.rot) + Vec3::splat(0.5));
        push_quad(out, verts, color);
    }
}

fn build_chunk_mesh(
    level: &LevelDocument,
    cpos: IVec3,
    manifest: &BlockAssetManifest,
) -> Option<Mesh> {
    let mut out = MeshOut {
        positions: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
    };

    let origin = cpos * CHUNK_SIZE;

    for lx in 0..CHUNK_SIZE {
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let cell = origin + IVec3::new(lx, ly, lz);
                let Some(block) = level.get_block(cell) else {
                    continue;
                };
                append_block(&mut out, level, cell, block, manifest);
            }
        }
    }

    if out.indices.is_empty() {
        return None;
    }
    Some(finish_mesh(out))
}

/// Translucent water-block geometry for one chunk (Water blocks are always
/// full cubes, tinted by the level theme's water color).
fn build_water_mesh(level: &LevelDocument, cpos: IVec3) -> Option<Mesh> {
    let mut out = MeshOut {
        positions: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
    };

    let origin = cpos * CHUNK_SIZE;
    let water = theme::theme_env(level.data.theme)
        .water
        .to_linear()
        .to_f32_array();
    let color = [water[0], water[1], water[2], 0.72];

    for lx in 0..CHUNK_SIZE {
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let cell = origin + IVec3::new(lx, ly, lz);
                let Some(block) = level.get_block(cell) else {
                    continue;
                };
                if block.kind != BlockKind::Water {
                    continue;
                }
                for f in shape_faces(BlockShape::Full) {
                    if f.dir != IVec3::ZERO {
                        let neighbor = level.get_block(cell + f.dir);
                        if neighbor.is_some_and(|nb| face_covered_by_neighbor(level, nb)) {
                            continue;
                        }
                    }
                    let verts = f.verts.map(|p| cell.as_vec3() + p + Vec3::splat(0.5));
                    push_quad(&mut out, verts, color);
                }
            }
        }
    }

    if out.indices.is_empty() {
        return None;
    }
    Some(finish_mesh(out))
}

pub fn rebuild_dirty_chunks(
    mut commands: Commands,
    mut level: ResMut<LevelDocument>,
    assets: Option<Res<MakerAssets>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: ResMut<ChunkEntities>,
    mut water_chunks: ResMut<WaterChunkEntities>,
) {
    let Some(assets) = assets else {
        return;
    };
    if level.dirty_chunks.is_empty() && !level.is_changed() {
        return;
    }

    let dirty: Vec<IVec3> = level.dirty_chunks.drain().collect();

    for cpos in dirty {
        match build_chunk_mesh(&level, cpos, &assets.block_manifest) {
            Some(mesh) => {
                let handle = meshes.add(mesh);
                match chunks.0.get(&cpos) {
                    Some(&e) => {
                        commands.entity(e).insert(Mesh3d(handle));
                    }
                    None => {
                        let e = commands
                            .spawn((
                                Mesh3d(handle),
                                MeshMaterial3d(assets.chunk_material.clone()),
                                Transform::IDENTITY,
                                MakerCleanup,
                            ))
                            .id();
                        chunks.0.insert(cpos, e);
                    }
                }
            }
            None => {
                if let Some(e) = chunks.0.remove(&cpos) {
                    commands.entity(e).despawn();
                }
            }
        }
        match build_water_mesh(&level, cpos) {
            Some(mesh) => {
                let handle = meshes.add(mesh);
                match water_chunks.0.get(&cpos) {
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
                        water_chunks.0.insert(cpos, e);
                    }
                }
            }
            None => {
                if let Some(e) = water_chunks.0.remove(&cpos) {
                    commands.entity(e).despawn();
                }
            }
        }
    }

    let has_content = |level: &LevelDocument, cpos: IVec3| -> bool {
        let origin = cpos * CHUNK_SIZE;
        level.map.keys().any(|k| {
            let d = *k - origin;
            (0..CHUNK_SIZE).contains(&d.x)
                && (0..CHUNK_SIZE).contains(&d.y)
                && (0..CHUNK_SIZE).contains(&d.z)
        })
    };
    let has_water = |level: &LevelDocument, cpos: IVec3| -> bool {
        let origin = cpos * CHUNK_SIZE;
        level.map.iter().any(|(k, b)| {
            b.kind == BlockKind::Water && {
                let d = *k - origin;
                (0..CHUNK_SIZE).contains(&d.x)
                    && (0..CHUNK_SIZE).contains(&d.y)
                    && (0..CHUNK_SIZE).contains(&d.z)
            }
        })
    };

    chunks.0.retain(|cpos, e| {
        if !has_content(&level, *cpos) {
            commands.entity(*e).despawn();
        }
        has_content(&level, *cpos)
    });
    water_chunks.0.retain(|cpos, e| {
        if !has_water(&level, *cpos) {
            commands.entity(*e).despawn();
        }
        has_water(&level, *cpos)
    });
}

/// Spawns/despawns the real pack-model scenes that visually replace the
/// procedural cube for block kind×shape pairs with a `BlockAssetManifest`
/// model. The overlay is a child of a per-cell root so the cell transform
/// matches chunk rendering. Runs every frame (cheap retain), so it never
/// depends on the dirty set that `rebuild_dirty_chunks` drains.
pub fn reconcile_block_overlays(
    mut commands: Commands,
    level: Res<LevelDocument>,
    assets: Option<Res<MakerAssets>>,
    mut overlays: ResMut<BlockOverlayEntities>,
    meta_q: Query<&BlockOverlayMeta>,
) {
    let Some(assets) = assets else {
        return;
    };

    // Despawn overlays that no longer match the cell: the block went away,
    // the pulse turned off, the pack model disappeared, or the kind/shape/rot
    // changed (stale art must not linger).
    overlays.0.retain(|cell, e| {
        let keep = level.map.get(cell).is_some_and(|b| {
            if b.kind.is_pulse() && !level.pulse_on {
                return false;
            }
            if overlay_model(&assets.block_manifest, b.kind, b.shape).is_none() {
                return false;
            }
            match meta_q.get(*e) {
                Ok(meta) => meta.kind == b.kind && meta.shape == b.shape && meta.rot == b.rot,
                // Missing meta → rebuild.
                Err(_) => false,
            }
        });
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });

    // Spawn overlays for cells missing one.
    for (cell, block) in &level.map {
        let kind = block.kind;
        let shape = block.shape;
        if block.kind.is_pulse() && !level.pulse_on {
            continue;
        }
        let Some((scale, y_off, tint)) =
            overlay_placement(&assets.block_manifest, kind, shape)
        else {
            continue;
        };
        if overlays.0.contains_key(cell) {
            continue;
        }
        let Some(scene) = assets.block_overlays.get(&(kind, shape)) else {
            continue;
        };
        let scene = scene.clone();
        let yaw = block.rot as f32 * std::f32::consts::FRAC_PI_2;
        let root = commands
            .spawn((
                Transform::from_translation(cell.as_vec3() + Vec3::splat(0.5))
                    .with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::default(),
                MakerCleanup,
                BlockOverlayMeta {
                    kind,
                    shape,
                    rot: block.rot,
                },
            ))
            .id();
        commands.entity(root).with_children(|p| {
            let mut child = p.spawn((
                WorldAssetRoot(scene),
                MakerCleanup,
                Visibility::default(),
                Transform::from_translation(Vec3::new(0.0, y_off, 0.0))
                    .with_scale(Vec3::splat(scale)),
            ));
            // Only force a flat material when the pack is meant to be tinted.
            // Model tint keeps the glTF albedo (apply_model_materials is a
            // fallback that only fills meshes lacking MeshMaterial3d).
            match tint {
                BlockTintMode::Model => {
                    child.insert(ModelMaterial(assets.model_inert_mat.clone()));
                }
                BlockTintMode::Kind | BlockTintMode::Theme | BlockTintMode::Link => {
                    child.insert(ModelMaterial(assets.ghost_mats[&kind].clone()));
                }
            }
        });
        overlays.0.insert(*cell, root);
    }
}

pub fn spawn_place_ghost(
    commands: &mut Commands,
    assets: &MakerAssets,
    cell: IVec3,
    kind: BlockKind,
    shape: BlockShape,
    rot: u8,
) {
    let mesh = assets.shape_meshes[&shape].clone();
    let mat = assets
        .ghost_alpha_mats
        .get(&kind)
        .cloned()
        .unwrap_or_else(|| assets.ghost_mats[&kind].clone());
    let e = commands
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::from_translation(cell.as_vec3() + Vec3::splat(0.5))
                .with_rotation(Quat::from_rotation_y(
                    rot as f32 * std::f32::consts::FRAC_PI_2,
                ))
                .with_scale(Vec3::splat(1.04)),
            GhostTimer(0.25),
            MakerCleanup,
        ))
        .id();
    game_utils_bevy::juice::Juice::pop_in(commands, e, 0.18);
}

pub fn tick_ghosts(
    time: Res<Time>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut GhostTimer)>,
) {
    for (e, mut t) in &mut q {
        t.0 -= time.delta_secs();
        if t.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

/// Rebuilds the water surface plane and boundary walls when the level's water
/// level / size / theme / boundary config changes. Boundary walls (and the
/// ceiling) are a level-design aid, so they're only shown in Edit mode.
pub fn rebuild_water_and_boundary(
    mut commands: Commands,
    mut level: ResMut<LevelDocument>,
    mut state: ResMut<WaterBoundaryState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    water_q: Query<Entity, With<WaterSurface>>,
    wall_q: Query<Entity, With<BoundaryWall>>,
    mode: Res<super::mode::MakerMode>,
) {
    let size = level.play_size();
    let water_level = level.water_level();
    let walls = level.data.boundary.inner_walls || level.data.boundary.outer_walls;
    let floor = level.data.boundary.inner_floor || level.data.boundary.outer_floor;
    let ceiling = level.data.boundary.ceiling;
    let theme = level.data.theme;
    let edit = *mode == super::mode::MakerMode::Edit;
    let changed = (water_level, size, walls, floor, ceiling, theme, edit)
        != (
            state.water_level,
            state.size,
            state.walls,
            state.floor,
            state.ceiling,
            state.theme,
            state.edit,
        );

    if changed {
        // Re-tint chunk geometry (underwater shading) when the water level or
        // theme moves, and rebuild the water/boundary entities.
        if water_level != state.water_level || theme != state.theme {
            level.mark_all_dirty();
        }

        for e in &water_q {
            commands.entity(e).despawn();
        }
        for e in &wall_q {
            commands.entity(e).despawn();
        }

        if let Some(wl) = water_level {
            let env = theme::theme_env(theme);
            let mut mat = StandardMaterial::from_color(env.water);
            mat.alpha_mode = AlphaMode::Blend;
            mat.perceptual_roughness = 0.15;
            let mut plane = Plane3d::default();
            plane.half_size = Vec2::new(size[0] as f32 + 0.5, size[2] as f32 + 0.5);
            commands.spawn((
                Mesh3d(meshes.add(plane.mesh())),
                MeshMaterial3d(materials.add(mat)),
                Transform::from_xyz(0.0, wl as f32, 0.0),
                WaterSurface,
                MakerCleanup,
            ));
        }

        if walls && edit {
            let (min, max) = level.play_bounds();
            let mut mat = StandardMaterial::from_color(Color::srgba(0.7, 0.3, 0.85, 0.35));
            mat.alpha_mode = AlphaMode::Blend;
            mat.perceptual_roughness = 0.6;
            let handle = materials.add(mat);
            let h = (max.y - min.y + 1) as f32;
            let len = size[2] as f32 * 2.0 + 1.0;
            for x in [min.x as f32 - 0.5, max.x as f32 + 0.5] {
                commands
                    .spawn((
                        Mesh3d(meshes.add(Cuboid::new(0.1, h, len).mesh())),
                        MeshMaterial3d(handle.clone()),
                        Transform::from_xyz(x, (max.y + min.y) as f32 * 0.5, 0.0),
                        BoundaryWall,
                        MakerCleanup,
                    ))
                    .insert(Visibility::Visible);
            }
            let len = size[0] as f32 * 2.0 + 1.0;
            for z in [min.z as f32 - 0.5, max.z as f32 + 0.5] {
                commands
                    .spawn((
                        Mesh3d(meshes.add(Cuboid::new(len, h, 0.1).mesh())),
                        MeshMaterial3d(handle.clone()),
                        Transform::from_xyz(0.0, (max.y + min.y) as f32 * 0.5, z),
                        BoundaryWall,
                        MakerCleanup,
                    ))
                    .insert(Visibility::Visible);
            }
        }

        if floor && edit {
            let mut mat = StandardMaterial::from_color(Color::srgba(0.7, 0.3, 0.85, 0.3));
            mat.alpha_mode = AlphaMode::Blend;
            mat.perceptual_roughness = 0.6;
            let handle = materials.add(mat);
            let len_x = size[0] as f32 * 2.0 + 1.0;
            let len_z = size[2] as f32 * 2.0 + 1.0;
            commands
                .spawn((
                    Mesh3d(meshes.add(Cuboid::new(len_x, 0.1, len_z).mesh())),
                    MeshMaterial3d(handle),
                    Transform::from_xyz(0.0, -0.5, 0.0),
                    BoundaryWall,
                    MakerCleanup,
                ))
                .insert(Visibility::Visible);
        }

        if ceiling && edit {
            let mut mat = StandardMaterial::from_color(Color::srgba(0.7, 0.3, 0.85, 0.35));
            mat.alpha_mode = AlphaMode::Blend;
            mat.perceptual_roughness = 0.6;
            let handle = materials.add(mat);
            let len_x = size[0] as f32 * 2.0 + 1.0;
            let len_z = size[2] as f32 * 2.0 + 1.0;
            commands
                .spawn((
                    Mesh3d(meshes.add(Cuboid::new(len_x, 0.1, len_z).mesh())),
                    MeshMaterial3d(handle),
                    Transform::from_xyz(0.0, level.boundary_top() as f32 + 0.5, 0.0),
                    BoundaryWall,
                    MakerCleanup,
                ))
                .insert(Visibility::Visible);
        }
    }

    state.water_level = water_level;
    state.size = size;
    state.walls = walls;
    state.floor = floor;
    state.ceiling = ceiling;
    state.theme = theme;
    state.edit = edit;
}

/// Applies the level theme to the camera clear color and ambient light.
pub fn apply_theme(
    mut ambient: ResMut<GlobalAmbientLight>,
    level: Res<LevelDocument>,
    mut cam_q: Query<&mut Camera, With<super::camera::WorldCamera>>,
) {
    if !level.is_changed() {
        return;
    }
    let env = theme::theme_env(level.data.theme);
    for mut cam in &mut cam_q {
        cam.clear_color = bevy::camera::ClearColorConfig::Custom(env.sky);
    }
    ambient.color = Color::WHITE;
    ambient.brightness = env.ambient;
}

pub fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    level: Res<LevelDocument>,
    manifest: Res<BlockAssetManifest>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    let mut chunk_mat = StandardMaterial::from_color(Color::WHITE);
    chunk_mat.perceptual_roughness = 0.9;
    let chunk_material = materials.add(chunk_mat);

    let player_scene = asset_server.load("models/cubeworld/Character_Male_2.gltf#Scene0");

    // Pack model ships its own materials; this is only an inert fallback.
    let player_material = materials.add(StandardMaterial {
        perceptual_roughness: 0.9,
        ..default()
    });

    let mut preview = StandardMaterial::from_color(Color::srgba(1.0, 1.0, 1.0, 0.35));
    preview.alpha_mode = AlphaMode::Blend;
    let preview_mat = materials.add(preview);

    let mut water = StandardMaterial::from_color(Color::srgba(0.25, 0.6, 0.95, 0.72));
    water.alpha_mode = AlphaMode::Blend;
    water.perceptual_roughness = 0.15;
    let water_material = materials.add(water);

    let mut ghost_mats = HashMap::new();
    for kind in ALL_BLOCK_KINDS {
        ghost_mats.insert(
            *kind,
            materials.add(StandardMaterial::from_color(kind.color())),
        );
    }

    let mut ghost_alpha_mats = HashMap::new();
    for kind in ALL_BLOCK_KINDS {
        let c = kind.color().to_srgba();
        let mut m = StandardMaterial::from_color(Color::srgba(c.red, c.green, c.blue, 0.45));
        m.alpha_mode = AlphaMode::Blend;
        m.perceptual_roughness = 0.85;
        ghost_alpha_mats.insert(*kind, materials.add(m));
    }

    // Inert fallback for pack models whose scenes ship albedo textures. Only
    // applied to meshes that truly lack a MeshMaterial3d (apply_model_materials),
    // so `tint: Model` packs never get their albedo wiped by a flat color.
    let model_inert_mat = materials.add(StandardMaterial {
        perceptual_roughness: 0.9,
        ..default()
    });

    let mut shape_meshes = HashMap::new();
    for shape in ALL_BLOCK_SHAPES {
        shape_meshes.insert(*shape, meshes.add(build_shape_mesh(*shape)));
    }

    let mut block_overlays = HashMap::new();
    for kind in ALL_BLOCK_KINDS {
        for shape in ALL_BLOCK_SHAPES {
            if let Some(path) = overlay_model(&manifest, *kind, *shape) {
                block_overlays.insert(
                    (*kind, *shape),
                    asset_server.load(path.to_owned()),
                );
            }
        }
    }

    let assets = MakerAssets {
        chunk_material,
        water_material,
        player_scene,
        player_material,
        preview_mat: preview_mat.clone(),
        ghost_mats,
        ghost_alpha_mats,
        model_inert_mat,
        shape_meshes,
        block_overlays,
        block_manifest: manifest.clone(),
    };

    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(assets.preview_mat.clone()),
        Transform::from_scale(Vec3::splat(1.02)),
        Visibility::Hidden,
        PlacementPreview,
        MakerCleanup,
    ));

    player::spawn_player(&mut commands, &assets, &level);

    let env = theme::theme_env(level.data.theme);
    commands.insert_resource(GlobalAmbientLight {
        color: Color::WHITE,
        brightness: env.ambient,
        ..default()
    });

    commands.insert_resource(assets);
}
