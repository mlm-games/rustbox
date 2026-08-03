use bevy::prelude::*;

use super::level::LevelDocument;

/// Editor soft limits for a level (a UX guide; the wire format keeps its own
/// harder safety caps in `rustbox_format::file`).
#[derive(Resource, Clone, Debug)]
pub struct LevelLimits {
    pub max_blocks: u32,
    pub max_entities: u32,
    pub max_tracks: u32,
    pub max_track_points: u32,
    pub max_estimated_vertices: u32,
    /// Fraction of a limit at which the HUD switches to warning color.
    pub warn_ratio: f32,
}

impl Default for LevelLimits {
    fn default() -> Self {
        Self {
            max_blocks: 20_000,
            max_entities: 1_000,
            max_tracks: 200,
            max_track_points: 2_000,
            max_estimated_vertices: 80_000,
            warn_ratio: 0.8,
        }
    }
}

/// Live counts derived from the document each frame (cheap: map/vec lengths
/// plus an exposed-face estimate for blocks).
#[derive(Resource, Default, Clone, Debug)]
pub struct LevelStats {
    pub blocks: u32,
    pub entities: u32,
    pub tracks: u32,
    pub track_points: u32,
    pub estimated_vertices: u32,
    pub warning: bool,
    pub over_limit: bool,
}

pub fn update_level_stats(
    level: Res<LevelDocument>,
    limits: Res<LevelLimits>,
    mut stats: ResMut<LevelStats>,
) {
    stats.blocks = level.map.len() as u32;
    stats.entities = level.data.entities.len() as u32;
    stats.tracks = level.data.tracks.len() as u32;
    stats.track_points = level
        .data
        .tracks
        .iter()
        .map(|t| t.points.len() as u32)
        .sum();

    // Conservative vertex estimate: each exposed face of a placed block gets 4
    // vertices. Boundary walls/floors are ignored (they don't add real blocks).
    let mut faces = 0u32;
    for cell in level.map.keys() {
        for n in [
            IVec3::X,
            IVec3::NEG_X,
            IVec3::Y,
            IVec3::NEG_Y,
            IVec3::Z,
            IVec3::NEG_Z,
        ] {
            if !level.map.contains_key(&(*cell + n)) {
                faces += 1;
            }
        }
    }
    stats.estimated_vertices = faces * 4;

    stats.warning = stats.blocks as f32 > limits.max_blocks as f32 * limits.warn_ratio
        || stats.entities as f32 > limits.max_entities as f32 * limits.warn_ratio
        || stats.estimated_vertices as f32
            > limits.max_estimated_vertices as f32 * limits.warn_ratio;

    stats.over_limit = stats.blocks > limits.max_blocks
        || stats.entities > limits.max_entities
        || stats.tracks > limits.max_tracks
        || stats.track_points > limits.max_track_points
        || stats.estimated_vertices > limits.max_estimated_vertices;
}
