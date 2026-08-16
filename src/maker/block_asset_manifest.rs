//! Data-driven block asset manifest.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::Resource;
use rustbox_format::{ALL_BLOCK_SHAPES, BlockKind, BlockShape};
use serde::{Deserialize, Serialize};

/// How a glTF model's materials are (re)tinted.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum BlockTintMode {
    /// Leave the model's own albedo textures / materials untouched.
    Model,
    /// Flat `BlockKind::color()` material (kind color language).
    Kind,
    /// Level theme color.
    Theme,
    /// Link-channel color (1-9).
    Link,
}

/// Collider reference for play mode (mirrors `bevy_rapier3d` shapes).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum BlockColliderRef {
    /// Use the engine's own shape math (the collision sampler); no rapier
    /// collider is spawned.
    BuiltinShape,
    /// `Collider::cuboid(hx, hy, hz)`.
    Box(f32, f32, f32),
    /// Sloped-top triangular prism; `(hx, hy, hz)` is its bounding box.
    Wedge(f32, f32, f32),
    /// Baked mesh collider; the path names a `.glb`/`.gltf` under `models/`.
    Mesh(String),
}

/// Visual + preview configuration for one block kind×shape pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockAssetEntry {
    /// glTF scene path loaded via the `AssetServer` (e.g.
    /// `models/cubeworld/Character_Male_2.gltf#Scene0`) or `None` for the
    /// procedural chunk mesh.
    pub model: Option<String>,
    /// Palette preview PNG path under `assets/` (e.g.
    /// `images/previews/blocks/grass_full.png`).
    pub preview: String,
    /// Visual scale applied to a spawned model root.
    pub scale: f32,
    /// Vertical offset applied to a spawned model root.
    pub y_offset: f32,
    pub tint: BlockTintMode,
    pub collider: BlockColliderRef,
    /// Axis-aligned visual bounds `[x, y, z]` of the shape footprint.
    pub visual_bounds: [f32; 3],
}

/// Full per-pair manifest, keyed by `"{kind:?}/{shape:?}"`.
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub struct BlockAssetManifest {
    pub blocks: HashMap<String, BlockAssetEntry>,
}

impl Default for BlockAssetManifest {
    fn default() -> Self {
        Self::defaults()
    }
}

/// Stable string key for a block kind×shape pair (what the RON uses).
pub fn block_asset_key(kind: BlockKind, shape: BlockShape) -> String {
    format!("{kind:?}/{shape:?}")
}

/// Palette preview file base name for a block kind (`"grass"`, ...). Shapes
/// without a dedicated preview reuse the kind's PNG.
pub fn block_preview_base(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Grass => "grass",
        BlockKind::Stone => "stone",
        BlockKind::Hazard => "hazard",
        BlockKind::Goal => "goal",
        BlockKind::Spawn => "spawn",
        BlockKind::Water => "water",
        BlockKind::Ice => "ice",
        BlockKind::Spikes => "spikes",
        BlockKind::Conveyor => "conveyor",
        BlockKind::Bounce => "bounce",
        BlockKind::Climb => "climb",
        BlockKind::ThinConveyor => "thin_conveyor",
        BlockKind::OnOffConveyorA => "onoff_conveyor_a",
        BlockKind::OnOffConveyorB => "onoff_conveyor_b",
        BlockKind::HangRail => "hang_rail",
        BlockKind::OneWay => "one_way",
        BlockKind::TimedPulse => "timed_pulse",
    }
}

/// Rustbox pack model path for a block kind×shape pair, cell-aligned and
/// centered on the origin (mirrors `tools/asset_build/gen_manifests.py`).
/// Returns `None` for Water, which stays a translucent procedural chunk mesh (but still fallback atm)
pub fn block_overlay_model(kind: BlockKind, shape: BlockShape) -> Option<String> {
    if kind == BlockKind::Water {
        return None;
    }
    Some(format!(
        "models/rustbox/blocks/{kind:?}/{kind:?}_{shape:?}.glb#Scene0"
    ))
}

/// Default visual bounds of a shape footprint (used for preview framing).
fn shape_bounds(shape: BlockShape) -> [f32; 3] {
    match shape {
        BlockShape::Half | BlockShape::TopHalf => [1.0, 0.5, 1.0],
        BlockShape::VerticalSlab => [1.0, 1.0, 0.5],
        BlockShape::Thin => [1.0, rustbox_format::block::THIN_HEIGHT, 1.0],
        _ => [1.0, 1.0, 1.0],
    }
}

impl BlockAssetManifest {
    pub fn entry(&self, kind: BlockKind, shape: BlockShape) -> Option<&BlockAssetEntry> {
        self.blocks.get(&block_asset_key(kind, shape))
    }

    /// The entry that supplies a real model for `kind` (any shape), matching
    /// the historical sparse overlays: `{kind}/Full`, if it has a model.
    pub fn overlay(&self, kind: BlockKind) -> Option<&BlockAssetEntry> {
        self.entry(kind, BlockShape::Full)
            .filter(|e| e.model.is_some())
    }

    /// Palette preview path for a pair, falling back to the kind's default PNG
    /// when the pair lacks one.
    pub fn preview(&self, kind: BlockKind, shape: BlockShape) -> String {
        self.entry(kind, shape)
            .map(|e| e.preview.clone())
            .unwrap_or_else(|| format!("images/blocks/{}.png", block_preview_base(kind)))
    }

    /// Load the manifest from disk, merging over the built-in defaults so a
    /// partial pack RON never blanks the rest of the catalog. Falls back to
    /// the defaults when the file is missing or malformed (the game must run
    /// either way).
    pub fn load(asset_root: &Path) -> Self {
        let path = asset_root.join("models/blocks.ron");
        let disk = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| ron::from_str::<Self>(&text).ok());
        match disk {
            Some(other) => Self::defaults().merged_with(other),
            None => Self::defaults(),
        }
    }

    pub fn defaults() -> Self {
        let mut blocks = HashMap::new();
        for kind in [
            BlockKind::Grass,
            BlockKind::Stone,
            BlockKind::Hazard,
            BlockKind::Goal,
            BlockKind::Spawn,
            BlockKind::Water,
            BlockKind::Ice,
            BlockKind::Spikes,
            BlockKind::Conveyor,
            BlockKind::Bounce,
            BlockKind::Climb,
            BlockKind::ThinConveyor,
            BlockKind::OnOffConveyorA,
            BlockKind::OnOffConveyorB,
            BlockKind::HangRail,
            BlockKind::OneWay,
            BlockKind::TimedPulse,
        ] {
            for &shape in ALL_BLOCK_SHAPES {
                let model = block_overlay_model(kind, shape);
                let tint = if model.is_some() {
                    BlockTintMode::Model
                } else {
                    BlockTintMode::Kind
                };
                blocks.insert(
                    block_asset_key(kind, shape),
                    BlockAssetEntry {
                        model,
                        preview: format!("images/blocks/{}.png", block_preview_base(kind)),
                        scale: 1.0,
                        y_offset: 0.0,
                        tint,
                        collider: BlockColliderRef::BuiltinShape,
                        visual_bounds: shape_bounds(shape),
                    },
                );
            }
        }
        Self { blocks }
    }

    /// Merge entries from `other` (e.g. a RON override) over the defaults.
    /// Pairs present in `other` replace their default entry entirely.
    pub fn merged_with(mut self, other: Self) -> Self {
        self.blocks.extend(other.blocks);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_every_kind_shape_pair() {
        let m = BlockAssetManifest::defaults();
        let mut count = 0;
        for kind in [
            BlockKind::Grass,
            BlockKind::Stone,
            BlockKind::Hazard,
            BlockKind::Goal,
            BlockKind::Spawn,
            BlockKind::Water,
            BlockKind::Ice,
            BlockKind::Spikes,
            BlockKind::Conveyor,
            BlockKind::Bounce,
            BlockKind::Climb,
            BlockKind::ThinConveyor,
            BlockKind::OnOffConveyorA,
            BlockKind::OnOffConveyorB,
            BlockKind::HangRail,
            BlockKind::OneWay,
            BlockKind::TimedPulse,
        ] {
            for &shape in ALL_BLOCK_SHAPES {
                assert!(m.entry(kind, shape).is_some(), "missing {kind:?}/{shape:?}");
                count += 1;
            }
        }
        assert_eq!(count, m.blocks.len(), "expected no extra keys");
    }

    #[test]
    fn keys_are_stable_and_unique() {
        let mut keys = std::collections::HashSet::new();
        for kind in [
            BlockKind::Grass,
            BlockKind::Hazard,
            BlockKind::ThinConveyor,
            BlockKind::OnOffConveyorA,
            BlockKind::TimedPulse,
        ] {
            for &shape in ALL_BLOCK_SHAPES {
                assert!(keys.insert(block_asset_key(kind, shape)));
            }
        }
        assert_eq!(
            block_asset_key(BlockKind::Grass, BlockShape::Full),
            "Grass/Full"
        );
        assert_eq!(
            block_asset_key(BlockKind::ThinConveyor, BlockShape::Slope),
            "ThinConveyor/Slope"
        );
    }

    #[test]
    fn every_preview_exists_under_assets() {
        let m = BlockAssetManifest::defaults();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for (key, entry) in &m.blocks {
            let p = root.join(&entry.preview);
            assert!(p.exists(), "{key}: preview missing at {}", p.display());
            if let Some(model) = &entry.model {
                // "#Scene0" is a Bevy scene label, not a file path.
                let path = model.split('#').next().unwrap();
                let mp = root.join(path);
                assert!(mp.exists(), "{key}: model missing at {}", mp.display());
            }
        }
    }

    #[test]
    fn disk_manifest_parses_and_roundtrips_with_defaults() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/models/blocks.ron");
        let text = std::fs::read_to_string(&path).expect("blocks.ron exists");
        let disk: BlockAssetManifest = ron::from_str(&text).expect("blocks.ron parses");
        let merged = BlockAssetManifest::defaults().merged_with(disk);
        for kind in [
            BlockKind::Grass,
            BlockKind::Hazard,
            BlockKind::Goal,
            BlockKind::TimedPulse,
        ] {
            for &shape in ALL_BLOCK_SHAPES {
                assert!(
                    merged.entry(kind, shape).is_some(),
                    "merged lost {kind:?}/{shape:?}"
                );
            }
        }
        // Landmark kinds keep their pack models after the merge.
        for kind in [
            BlockKind::Goal,
            BlockKind::Bounce,
            BlockKind::Hazard,
            BlockKind::Spikes,
        ] {
            assert!(merged.overlay(kind).is_some(), "{kind:?} overlay lost");
        }
    }

    #[test]
    fn every_default_block_model_path_matches_pack_layout() {
        let m = BlockAssetManifest::defaults();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for (key, e) in &m.blocks {
            if let Some(model) = &e.model {
                let path = model.split('#').next().unwrap();
                let p = root.join(path);
                assert!(p.exists(), "{key} -> model missing at {}", p.display());
            }
        }
    }
}
