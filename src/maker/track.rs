use bevy::prelude::*;

use super::level::LevelDocument;
use super::mode::MakerMode;

pub use rustbox_format::track::{TrackData, TrackId, TrackMode};

/// Bevy-math helpers for track data (the pure `TrackData` struct lives in
/// `rustbox-format`; these need `Vec3`).
pub trait TrackDataExt {
    fn world_points(&self) -> Vec<Vec3>;
    fn total_length(&self) -> f32;
    /// Closest point to `p`: (arc-length distance, nearest point, distance to track).
    fn nearest(&self, p: Vec3) -> Option<(f32, Vec3, f32)>;
    /// Position at distance `d` along the track, honoring `mode`.
    fn sample(&self, d: f32) -> Vec3;
}

impl TrackDataExt for TrackData {
    fn world_points(&self) -> Vec<Vec3> {
        self.points
            .iter()
            .map(|p| IVec3::from_array(*p).as_vec3() + Vec3::new(0.5, 0.15, 0.5))
            .collect()
    }

    fn total_length(&self) -> f32 {
        let pts = self.world_points();
        let mut len = pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
        if self.mode == TrackMode::Loop && pts.len() > 2 {
            len += pts.last().unwrap().distance(pts[0]);
        }
        len.max(0.001)
    }

    fn nearest(&self, p: Vec3) -> Option<(f32, Vec3, f32)> {
        let pts = self.world_points();
        if pts.is_empty() {
            return None;
        }
        if pts.len() == 1 {
            return Some((0.0, pts[0], p.distance(pts[0])));
        }
        let segs: Vec<(Vec3, Vec3)> = if self.mode == TrackMode::Loop {
            pts.windows(2)
                .map(|w| (w[0], w[1]))
                .chain(std::iter::once((*pts.last().unwrap(), pts[0])))
                .collect()
        } else {
            pts.windows(2).map(|w| (w[0], w[1])).collect()
        };
        let mut best: Option<(f32, Vec3, f32)> = None;
        let mut arc = 0.0;
        for (a, b) in segs {
            let seg = a.distance(b);
            let t = if seg > 0.0 {
                ((p - a).dot(b - a) / (seg * seg)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let closest = a.lerp(b, t);
            let d = p.distance(closest);
            if best.as_ref().is_none_or(|(_, _, bd)| d < *bd) {
                best = Some((arc + seg * t, closest, d));
            }
            arc += seg;
        }
        best
    }

    fn sample(&self, d: f32) -> Vec3 {
        let pts = self.world_points();
        if pts.is_empty() {
            return Vec3::ZERO;
        }
        if pts.len() == 1 {
            return pts[0];
        }

        let total = self.total_length();
        let d = match self.mode {
            TrackMode::Loop => d.rem_euclid(total),
            TrackMode::PingPong => {
                let cycle = d.rem_euclid(total * 2.0);
                if cycle <= total {
                    cycle
                } else {
                    total * 2.0 - cycle
                }
            }
        };

        let mut remaining = d;
        let seg_iter: Vec<(Vec3, Vec3)> = if self.mode == TrackMode::Loop && pts.len() > 2 {
            pts.windows(2)
                .map(|w| (w[0], w[1]))
                .chain(std::iter::once((*pts.last().unwrap(), pts[0])))
                .collect()
        } else {
            pts.windows(2).map(|w| (w[0], w[1])).collect()
        };

        for (a, b) in seg_iter {
            let seg = a.distance(b);
            if remaining <= seg {
                return a.lerp(b, if seg > 0.0 { remaining / seg } else { 0.0 });
            }
            remaining -= seg;
        }
        *pts.last().unwrap()
    }
}

/// The track currently being edited (Edit mode only).
#[derive(Resource, Default, Clone, Copy)]
pub struct ActiveTrack(pub Option<TrackId>);

pub fn draw_track_gizmos(
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    active: Res<ActiveTrack>,
    mut gizmos: Gizmos,
) {
    if *mode != MakerMode::Edit {
        return;
    }
    for track in &level.data.tracks {
        let pts = track.world_points();
        if pts.is_empty() {
            continue;
        }
        let selected = active.0 == Some(track.id);
        let color = if selected {
            Color::srgb(1.0, 0.9, 0.2)
        } else {
            Color::srgb(0.9, 0.55, 0.25)
        };
        for w in pts.windows(2) {
            gizmos.line(w[0], w[1], color);
        }
        if track.mode == TrackMode::Loop && pts.len() > 2 {
            gizmos.line(*pts.last().unwrap(), pts[0], color);
        }
        for p in &pts {
            gizmos.sphere(Isometry3d::from_translation(*p), 0.12, color);
        }
    }
}
