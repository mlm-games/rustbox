//! Play-area backdrop: the water surface plane and the boundary
//! walls / floor / ceiling the original drew as translucent design aids
//! (Bevy `rendering::rebuild_water_and_boundary`). The plane shows in both
//! modes; the boundary box is edit-only, same as the original.

use glam::Vec3;
use repame_view3d::{MeshGroup, Rgb};

use super::level::LevelDocument;
use super::level_view::srgb_to_linear;
use super::theme::theme_env;

const BOUNDARY_RGB: [f32; 3] = [0.7, 0.3, 0.85];
const WALL_ALPHA: f32 = 0.35;
const FLOOR_ALPHA: f32 = 0.3;

/// Linear clear color for the level's theme sky.
pub fn sky_color(level: &LevelDocument) -> [f32; 4] {
    let sky = theme_env(level.data.theme).sky;
    [sky[0], sky[1], sky[2], 1.0]
}

fn translucent(alpha: f32) -> MeshGroup {
    MeshGroup {
        depth_test: true,
        transparent: true,
        alpha,
        ..MeshGroup::default()
    }
}

fn box_at(group: &mut MeshGroup, center: Vec3, size: Vec3, color: Rgb, eye: Vec3) {
    group.push_box(
        center.x,
        center.y - size.y * 0.5,
        center.z,
        size.x,
        size.y,
        size.z,
        color,
        eye,
    );
}

/// Translucent backdrop geometry for one frame. `in_edit` gates the boundary
/// box (walls / floor / ceiling); the water plane always draws.
pub fn groups(level: &LevelDocument, in_edit: bool, eye: Vec3) -> Vec<MeshGroup> {
    let mut out = Vec::new();
    let size = level.play_size();
    let env = theme_env(level.data.theme);
    let color = srgb_to_linear(BOUNDARY_RGB);

    if let Some(water_level) = level.water_level() {
        let half_x = size[0] as f32 + 0.5;
        let half_z = size[2] as f32 + 0.5;
        let y = water_level as f32;
        let mut group = translucent(env.water_alpha);
        group.push_quad(
            [-half_x, y, -half_z],
            [-half_x, y, half_z],
            [half_x, y, half_z],
            [half_x, y, -half_z],
            env.water,
        );
        out.push(group);
    }

    if !in_edit {
        return out;
    }

    let b = &level.data.boundary;
    let (min, max) = level.play_bounds();
    let span_x = size[0] as f32 * 2.0 + 1.0;
    let span_z = size[2] as f32 * 2.0 + 1.0;

    if b.inner_walls || b.outer_walls {
        // Widen before the add: `boundary_top` comes straight from the level
        // file, and a huge declared height would overflow the i32.
        let height = ((max.y - min.y) as f64 + 1.0).max(0.0) as f32;
        let mid_y = (max.y + min.y) as f32 * 0.5;
        let mut group = translucent(WALL_ALPHA);
        for x in [min.x as f32 - 0.5, max.x as f32 + 0.5] {
            box_at(
                &mut group,
                Vec3::new(x, mid_y, 0.0),
                Vec3::new(0.1, height, span_z),
                color,
                eye,
            );
        }
        for z in [min.z as f32 - 0.5, max.z as f32 + 0.5] {
            box_at(
                &mut group,
                Vec3::new(0.0, mid_y, z),
                Vec3::new(span_x, height, 0.1),
                color,
                eye,
            );
        }
        out.push(group);
    }

    if b.inner_floor || b.outer_floor {
        let mut group = translucent(FLOOR_ALPHA);
        box_at(
            &mut group,
            Vec3::new(0.0, -0.5, 0.0),
            Vec3::new(span_x, 0.1, span_z),
            color,
            eye,
        );
        out.push(group);
    }

    if b.ceiling {
        let mut group = translucent(WALL_ALPHA);
        box_at(
            &mut group,
            Vec3::new(0.0, level.boundary_top() as f32 + 0.5, 0.0),
            Vec3::new(span_x, 0.1, span_z),
            color,
            eye,
        );
        out.push(group);
    }

    out
}