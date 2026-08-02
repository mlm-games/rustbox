use bevy::prelude::Color;

use super::block::{BlockKindColor, BlockShape};
use super::entity_data::EntityKindColor;
use super::level::{BlockData, LevelData};

// Internal render resolution (supersampled), then boxed down for the grid.
const RW: usize = 256;
const RH: usize = 192;

type Px = [u8; 4];

pub struct ThumbImage {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct ThumbPreview {
    pub cols: usize,
    pub rows: usize,
    /// row-major, cols*rows entries
    pub cells: Vec<Px>,
}

/// Render a level to a small color grid suitable for Repose colored-box display.
pub fn render_preview(level: &LevelData, cols: usize, rows: usize) -> ThumbPreview {
    let img = render(level);
    downsample_to_grid(&img, cols, rows)
}

/// Vertical extent of a shape inside its cell: (bottom, top) as fractions of
/// the cell height. Flat/extreme shapes draw as their tallest span; half
/// slabs and slopes get short drops so thumbnails read the real geometry.
fn block_heights(shape: BlockShape) -> (f32, f32) {
    match shape {
        BlockShape::Half => (0.0, 0.5),
        BlockShape::TopHalf => (0.5, 1.0),
        BlockShape::Slope | BlockShape::DSlope => (0.5, 1.0),
        _ => (0.0, 1.0),
    }
}

pub fn render(level: &LevelData) -> ThumbImage {
    let w = RW;
    let h = RH;
    let mut buf = vec![0u8; w * h * 4];

    // Soft sky gradient background (reads like a real level card).
    for y in 0..h {
        let t = y as f32 / (h as f32 - 1.0);
        let r = lerp(150.0, 205.0, t) as u8;
        let g = lerp(182.0, 220.0, t) as u8;
        let b = lerp(224.0, 238.0, t) as u8;
        for x in 0..w {
            let i = (y * w + x) * 4;
            buf[i] = r;
            buf[i + 1] = g;
            buf[i + 2] = b;
            buf[i + 3] = 255;
        }
    }

    if level.blocks.is_empty() && level.entities.is_empty() {
        return ThumbImage { w, h, rgba: buf };
    }

    // Occupancy for face culling.
    let mut occ = std::collections::HashMap::with_capacity(level.blocks.len());
    for b in &level.blocks {
        occ.insert((b.position[0], b.position[1], b.position[2]), b);
    }

    // Pass 1: fit. Unit projection a=1, b=0.5, c=1. Both axes scale uniformly.
    let (mut min_sx, mut min_sy, mut max_sx, mut max_sy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for b in &level.blocks {
        let cx = (b.position[0] - b.position[2]) as f32;
        let cy = (b.position[0] + b.position[2]) as f32 * 0.5 - b.position[1] as f32;
        min_sx = min_sx.min(cx - 1.0);
        max_sx = max_sx.max(cx + 1.0);
        min_sy = min_sy.min(cy - 0.5);
        max_sy = max_sy.max(cy + 1.5);
    }
    // Entity markers can sit above/outside the block footprint.
    for e in &level.entities {
        let cx = (e.cell[0] - e.cell[2]) as f32;
        let cy = (e.cell[0] + e.cell[2]) as f32 * 0.5 - e.cell[1] as f32;
        min_sx = min_sx.min(cx - 0.5);
        max_sx = max_sx.max(cx + 0.5);
        min_sy = min_sy.min(cy - 1.0);
        max_sy = max_sy.max(cy + 0.5);
    }
    let span_x = (max_sx - min_sx).max(0.001);
    let span_y = (max_sy - min_sy).max(0.001);
    let pad = 10.0;
    let scale = ((w as f32 - 2.0 * pad) / span_x).min((h as f32 - 2.0 * pad) / span_y);
    let a = scale;
    let bb = scale * 0.5;
    let c = scale;
    let free_x = (w as f32 - 2.0 * pad) - span_x * scale;
    let free_y = (h as f32 - 2.0 * pad) - span_y * scale;
    let ox = pad - min_sx * scale + free_x * 0.5;
    let oy = pad - min_sy * scale + free_y * 0.5;

    let project = |x: i32, y: i32, z: i32| -> (f32, f32) {
        let cx = (x - z) as f32;
        let cy = (x + z) as f32 * 0.5 - y as f32;
        (ox + cx * scale, oy + cy * scale)
    };

    // Pass 2: painter's order (back to front), draw culled cubes.
    let mut order: Vec<&BlockData> = level.blocks.iter().collect();
    order.sort_by_key(|b| (b.position[0] + b.position[2], b.position[1]));

    for b in order {
        let (x, y, z) = (b.position[0], b.position[1], b.position[2]);
        let (bot_h, top_h) = block_heights(b.shape);
        let above = occ.get(&(x, y + 1, z)).is_some_and(|nb| block_heights(nb.shape).0 == 0.0);
        let px = occ.get(&(x + 1, y, z)).is_some_and(|nb| block_heights(nb.shape).0 == 0.0);
        let pz = occ.get(&(x, y, z + 1)).is_some_and(|nb| block_heights(nb.shape).0 == 0.0);
        let up = above && top_h == 1.0;
        if up && px && pz {
            continue; // fully hidden
        }

        let (sx, sy0) = project(x, y, z);
        let sy = sy0 - (1.0 - top_h) * c;
        let drop = (top_h - bot_h) * c;
        let mut base = to_rgba(b.kind.color());
        if b.waterlogged {
            base = blend(base, to_rgba(Color::srgb(0.2, 0.55, 0.95)), 0.45);
        }

        let top = (sx, sy - bb);
        let right = (sx + a, sy);
        let bot = (sx, sy + bb);
        let left = (sx - a, sy);

        if !pz {
            fill_quad(
                &mut buf,
                w,
                h,
                left,
                bot,
                (bot.0, bot.1 + drop),
                (left.0, left.1 + drop),
                shade(base, 0.58),
            );
        }
        if !px {
            fill_quad(
                &mut buf,
                w,
                h,
                bot,
                right,
                (right.0, right.1 + drop),
                (bot.0, bot.1 + drop),
                shade(base, 0.80),
            );
        }
        if !up {
            fill_quad(&mut buf, w, h, top, right, bot, left, base);
        }
    }

    // Entities: bright markers hovering just above their cell top.
    for e in &level.entities {
        let (x, y, z) = (e.cell[0], e.cell[1], e.cell[2]);
        let (sx, sy0) = project(x, y, z);
        let sy = sy0 - c * 0.55;
        let col = to_rgba(e.kind.color());
        let r = (a * 0.42).max(2.5);
        fill_disc(&mut buf, w, h, sx, sy, r + 1.6, [22, 22, 28, 255]);
        fill_disc(&mut buf, w, h, sx, sy, r, col);
        fill_disc(
            &mut buf,
            w,
            h,
            sx - r * 0.28,
            sy - r * 0.28,
            r * 0.32,
            [255, 255, 255, 210],
        );
    }

    ThumbImage { w, h, rgba: buf }
}

fn downsample_to_grid(img: &ThumbImage, cols: usize, rows: usize) -> ThumbPreview {
    let mut cells = vec![[0u8; 4]; cols * rows];
    for gy in 0..rows {
        for gx in 0..cols {
            let x0 = gx * img.w / cols;
            let x1 = ((gx + 1) * img.w / cols).max(x0 + 1);
            let y0 = gy * img.h / rows;
            let y1 = ((gy + 1) * img.h / rows).max(y0 + 1);
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for y in y0..y1.min(img.h) {
                for x in x0..x1.min(img.w) {
                    let i = (y * img.w + x) * 4;
                    r += img.rgba[i] as u32;
                    g += img.rgba[i + 1] as u32;
                    b += img.rgba[i + 2] as u32;
                    n += 1;
                }
            }
            let n = n.max(1);
            cells[gy * cols + gx] = [(r / n) as u8, (g / n) as u8, (b / n) as u8, 255];
        }
    }
    ThumbPreview { cols, rows, cells }
}

fn fill_quad(
    buf: &mut [u8],
    w: usize,
    h: usize,
    p0: (f32, f32),
    p1: (f32, f32),
    p2: (f32, f32),
    p3: (f32, f32),
    c: Px,
) {
    fill_tri(buf, w, h, p0, p1, p2, c);
    fill_tri(buf, w, h, p0, p2, p3, c);
}

fn fill_tri(
    buf: &mut [u8],
    w: usize,
    h: usize,
    a: (f32, f32),
    b: (f32, f32),
    c: (f32, f32),
    color: Px,
) {
    let minx = a.0.min(b.0).min(c.0).floor().max(0.0) as i32;
    let maxx = a.0.max(b.0).max(c.0).ceil().min(w as f32 - 1.0) as i32;
    let miny = a.1.min(b.1).min(c.1).floor().max(0.0) as i32;
    let maxy = a.1.max(b.1).max(c.1).ceil().min(h as f32 - 1.0) as i32;
    if minx > maxx || miny > maxy {
        return;
    }
    let area = edge(a, b, c);
    if area.abs() < 1e-4 {
        return;
    }
    let inv = 1.0 / area;
    for py in miny..=maxy {
        for px in minx..=maxx {
            let p = (px as f32 + 0.5, py as f32 + 0.5);
            let w0 = edge(b, c, p) * inv;
            let w1 = edge(c, a, p) * inv;
            let w2 = edge(a, b, p) * inv;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                set_px(buf, w, px as usize, py as usize, color);
            }
        }
    }
}

fn fill_disc(buf: &mut [u8], w: usize, h: usize, cx: f32, cy: f32, r: f32, color: Px) {
    let minx = (cx - r).floor().max(0.0) as i32;
    let maxx = (cx + r).ceil().min(w as f32 - 1.0) as i32;
    let miny = (cy - r).floor().max(0.0) as i32;
    let maxy = (cy + r).ceil().min(h as f32 - 1.0) as i32;
    let r2 = r * r;
    for py in miny..=maxy {
        for px in minx..=maxx {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r2 {
                blend_px(buf, w, px as usize, py as usize, color);
            }
        }
    }
}

#[inline]
fn edge(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> f32 {
    (p.0 - a.0) * (b.1 - a.1) - (p.1 - a.1) * (b.0 - a.0)
}

#[inline]
fn set_px(buf: &mut [u8], w: usize, x: usize, y: usize, c: Px) {
    let i = (y * w + x) * 4;
    buf[i] = c[0];
    buf[i + 1] = c[1];
    buf[i + 2] = c[2];
    buf[i + 3] = 255;
}

#[inline]
fn blend_px(buf: &mut [u8], w: usize, x: usize, y: usize, c: Px) {
    let a = c[3] as f32 / 255.0;
    let i = (y * w + x) * 4;
    for k in 0..3 {
        buf[i + k] = (c[k] as f32 * a + buf[i + k] as f32 * (1.0 - a)) as u8;
    }
    buf[i + 3] = 255;
}

#[inline]
fn to_rgba(c: Color) -> Px {
    let s = c.to_srgba();
    [
        (s.red.clamp(0.0, 1.0) * 255.0) as u8,
        (s.green.clamp(0.0, 1.0) * 255.0) as u8,
        (s.blue.clamp(0.0, 1.0) * 255.0) as u8,
        255,
    ]
}

#[inline]
fn shade(c: Px, f: f32) -> Px {
    [
        (c[0] as f32 * f).min(255.0) as u8,
        (c[1] as f32 * f).min(255.0) as u8,
        (c[2] as f32 * f).min(255.0) as u8,
        255,
    ]
}

#[inline]
fn blend(a: Px, b: Px, t: f32) -> Px {
    [
        (lerp(a[0] as f32, b[0] as f32, t)).round() as u8,
        (lerp(a[1] as f32, b[1] as f32, t)).round() as u8,
        (lerp(a[2] as f32, b[2] as f32, t)).round() as u8,
        255,
    ]
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maker::block::{BlockKind, BlockShape};
    use crate::maker::entity_data::{EntityData, EntityDataExt, EntityKind};
    use crate::maker::level::BlockData;

    fn sample_level() -> LevelData {
        LevelData {
            name: "t".into(),
            spawn: [0, 1, 0],
            blocks: vec![
                BlockData {
                    position: [0, 0, 0],
                    kind: BlockKind::Grass,
                    shape: BlockShape::Full,
                    rot: 0,
                    waterlogged: false,
                },
                BlockData {
                    position: [1, 0, 0],
                    kind: BlockKind::Stone,
                    shape: BlockShape::Full,
                    rot: 0,
                    waterlogged: false,
                },
                BlockData {
                    position: [0, 1, 0],
                    kind: BlockKind::Goal,
                    shape: BlockShape::Full,
                    rot: 0,
                    waterlogged: false,
                },
            ],
            entities: vec![EntityData::defaults_for(
                EntityKind::Glimmer,
                bevy::prelude::IVec3::new(0, 2, 0),
                1,
            )],
            tracks: vec![],
            entities_version: 1,
            author_time: None,
            author_deaths: 0,
            is_verified: true,
            description: String::new(),
            tags: vec![],
            author: String::new(),
            created_at: 0,
            size: None,
            water_level: None,
            theme: rustbox_format::level::Theme::default(),
            boundary: Default::default(),
            secret_stars: 0,
            coin_star: false,
        }
    }

    #[test]
    fn preview_is_deterministic_and_filled() {
        let a = render_preview(&sample_level(), 18, 13);
        let b = render_preview(&sample_level(), 18, 13);
        assert_eq!(a.cols, 18);
        assert_eq!(a.rows, 13);
        assert_eq!(a.cells, b.cells, "same level => same preview");
        assert!(a.cells.iter().all(|c| c[3] == 255));

        let empty = render_preview(
            &LevelData {
                blocks: vec![],
                entities: vec![],
                ..sample_level()
            },
            18,
            13,
        );
        assert_ne!(a.cells, empty.cells, "blocks/entities change the image");
    }
}
