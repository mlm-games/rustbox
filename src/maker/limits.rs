use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use glam::IVec3;

use super::level::LevelDocument;

/// Editor soft limits for a level (a UX guide; the wire format keeps its own
/// harder safety caps in `rustbox_format::file`).
#[derive(Resource, Clone, Copy, Debug)]
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

pub fn update_level_stats(world: &mut World) {
    let limits = *world.resource::<LevelLimits>();
    let level = world.resource::<LevelDocument>();
    let blocks = level.map.len() as u32;
    let entities = level.data.entities.len() as u32;
    let tracks = level.data.tracks.len() as u32;
    let track_points: u32 = level
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
    let estimated_vertices = faces * 4;

    let warning = blocks as f32 > limits.max_blocks as f32 * limits.warn_ratio
        || entities as f32 > limits.max_entities as f32 * limits.warn_ratio
        || estimated_vertices as f32 > limits.max_estimated_vertices as f32 * limits.warn_ratio;

    let over_limit = blocks > limits.max_blocks
        || entities > limits.max_entities
        || tracks > limits.max_tracks
        || track_points > limits.max_track_points
        || estimated_vertices > limits.max_estimated_vertices;

    let mut stats = world.resource_mut::<LevelStats>();
    stats.blocks = blocks;
    stats.entities = entities;
    stats.tracks = tracks;
    stats.track_points = track_points;
    stats.estimated_vertices = estimated_vertices;
    stats.warning = warning;
    stats.over_limit = over_limit;
}
