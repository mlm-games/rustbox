use glam::{IVec3, Vec3};
use repame_view3d::{MeshGroup, Rgb};

use super::edit_ops::transformed_cell;
use super::entity_data::{EntityData, EntityDataExt, EntityKind, link_color};
use super::level::LevelDocument;
use super::level_view::srgb_to_linear;
use super::mode::{BoxFillStart, BrushTab, PastePreview, SelectedEntity, SelectionSet};
use super::track::{ActiveTrack, TrackDataExt, TrackMode};

const MAX_ITEMS: usize = 4096;
const WIRE_T: f32 = 0.04;

fn gizmo_group(alpha: f32) -> MeshGroup {
    MeshGroup {
        depth_test: true,
        transparent: true,
        alpha,
        ..MeshGroup::default()
    }
}

fn cube(group: &mut MeshGroup, center: Vec3, half: Vec3, color: Rgb, eye: Vec3) {
    group.push_box(
        center.x,
        center.y - half.y,
        center.z,
        half.x * 2.0,
        half.y * 2.0,
        half.z * 2.0,
        color,
        eye,
    );
}

fn wire_box(group: &mut MeshGroup, min: Vec3, max: Vec3, color: Rgb, eye: Vec3, t: f32) {
    let cx = (min.x + max.x) * 0.5;
    let cz = (min.z + max.z) * 0.5;
    for y in [min.y, max.y] {
        for z in [min.z, max.z] {
            group.push_box(cx, y - t * 0.5, z, max.x - min.x, t, t, color, eye);
        }
    }
    for x in [min.x, max.x] {
        for z in [min.z, max.z] {
            group.push_box(x, min.y - t * 0.5, z, t, max.y - min.y, t, color, eye);
        }
    }
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            group.push_box(x, y - t * 0.5, cz, t, t, max.z - min.z, color, eye);
        }
    }
}

fn ribbon(group: &mut MeshGroup, a: Vec3, b: Vec3, color: Rgb, width: f32) {
    let dir = b - a;
    if dir.length_squared() < 1e-8 {
        return;
    }
    let mut side = dir.cross(Vec3::Y);
    if side.length_squared() < 1e-6 {
        side = dir.cross(Vec3::X);
    }
    let side = side.normalize() * width;
    let p0 = a - side;
    let p1 = a + side;
    let p2 = b + side;
    let p3 = b - side;
    group.push_quad(
        p0.to_array(),
        p1.to_array(),
        p2.to_array(),
        p3.to_array(),
        color,
    );
    group.push_quad(
        p3.to_array(),
        p2.to_array(),
        p1.to_array(),
        p0.to_array(),
        color,
    );
}

fn push_nonempty(frame_groups: &mut Vec<MeshGroup>, group: MeshGroup) {
    if !group.positions.is_empty() {
        frame_groups.push(group);
    }
}

fn box_fill_groups(a: IVec3, b: IVec3, eye: Vec3) -> Vec<MeshGroup> {
    let min = a.min(b);
    let max = a.max(b);
    let lo = min.as_vec3();
    let hi = (max + IVec3::ONE).as_vec3();
    let size = hi - lo;
    let color = srgb_to_linear([0.95, 0.9, 0.2]);

    let mut outline = gizmo_group(0.95);
    wire_box(&mut outline, lo, hi, color, eye, WIRE_T);
    for corner in [
        lo,
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        hi,
    ] {
        cube(&mut outline, corner, Vec3::splat(0.08), color, eye);
    }

    let mut grid = gizmo_group(0.25);
    let steps = size.floor().min(Vec3::splat(48.0));
    let (sx, sz) = (steps.x as i32, steps.z as i32);
    let center_x = (lo.x + hi.x) * 0.5;
    let center_z = (lo.z + hi.z) * 0.5;
    if sx > 1 {
        for i in 1..sx {
            let x = lo.x + size.x * (i as f32 / sx as f32);
            grid.push_box(
                x,
                lo.y - WIRE_T * 0.5,
                center_z,
                WIRE_T,
                WIRE_T,
                size.z,
                color,
                eye,
            );
        }
    }
    if sz > 1 {
        for i in 1..sz {
            let z = lo.z + size.z * (i as f32 / sz as f32);
            grid.push_box(
                center_x,
                lo.y - WIRE_T * 0.5,
                z,
                size.x,
                WIRE_T,
                WIRE_T,
                color,
                eye,
            );
        }
    }

    vec![outline, grid]
}

fn entity_half(kind: EntityKind) -> Vec3 {
    use EntityKind::*;
    match kind {
        Glimmer | TriggerOrb | Checkpoint | Cannon => Vec3::splat(0.45),
        LaunchPad | Teleporter => Vec3::new(0.55, 0.3, 0.55),
        Seal | RelayGate => Vec3::new(0.6, 1.1, 0.4),
        DriftPlate => Vec3::new(0.8, 0.25, 0.8),
        Prowler | Fan | Crate | TossCrate | Wedge => Vec3::splat(0.5),
        Bumper => Vec3::splat(0.55),
        Key | HealOrb => Vec3::splat(0.4),
        LockGate => Vec3::new(0.55, 1.2, 0.3),
        SpeedRing => Vec3::splat(0.6),
        CrumblePlate => Vec3::new(0.55, 0.15, 0.55),
        OnOffSwitch => Vec3::new(0.35, 0.15, 0.35),
        Sign => Vec3::new(0.5, 1.0, 0.3),
    }
}

fn selection_groups(
    level: &LevelDocument,
    selection: &SelectionSet,
    selected: &SelectedEntity,
    eye: Vec3,
) -> Vec<MeshGroup> {
    let mut out = Vec::new();
    if !selection.is_empty() {
        let blocks: Vec<IVec3> = selection
            .blocks
            .iter()
            .copied()
            .filter(|c| level.get_block(*c).is_some())
            .collect();
        let entities: Vec<_> = selection
            .entities
            .iter()
            .filter_map(|id| level.entity_by_id(*id))
            .collect();
        if blocks.len() + entities.len() <= MAX_ITEMS {
            let block_color = srgb_to_linear([0.2, 0.9, 1.0]);
            let mut group = gizmo_group(0.45);
            for cell in &blocks {
                cube(
                    &mut group,
                    cell.as_vec3() + Vec3::splat(0.5),
                    Vec3::splat(0.54),
                    block_color,
                    eye,
                );
            }
            push_nonempty(&mut out, group);

            let entity_color = srgb_to_linear([1.0, 0.85, 0.25]);
            let mut group = gizmo_group(0.45);
            for entity in &entities {
                cube(
                    &mut group,
                    entity.cell_i().as_vec3() + Vec3::splat(0.5),
                    Vec3::splat(0.62),
                    entity_color,
                    eye,
                );
            }
            push_nonempty(&mut out, group);
        } else {
            let mut min = IVec3::splat(i32::MAX);
            let mut max = IVec3::splat(i32::MIN);
            for cell in &blocks {
                min = min.min(*cell);
                max = max.max(*cell);
            }
            for entity in &entities {
                let cell = entity.cell_i();
                min = min.min(cell);
                max = max.max(cell);
            }
            if min.x <= max.x {
                let lo = min.as_vec3();
                let hi = (max + IVec3::ONE).as_vec3();
                let mut group = gizmo_group(0.45);
                cube(
                    &mut group,
                    (lo + hi) * 0.5,
                    (hi - lo) * 0.5,
                    srgb_to_linear([0.2, 0.9, 1.0]),
                    eye,
                );
                push_nonempty(&mut out, group);
            }
        }
    }
    if let Some(id) = selected.0
        && let Some(entity) = level.entity_by_id(id)
    {
        let half = entity_half(entity.kind);
        let center = entity.cell_i().as_vec3() + Vec3::new(0.5, 0.0, 0.5);
        let mut group = gizmo_group(1.0);
        wire_box(
            &mut group,
            center - half,
            center + half,
            srgb_to_linear([0.3, 0.8, 1.0]),
            eye,
            WIRE_T,
        );
        push_nonempty(&mut out, group);
    }
    out
}

fn paste_groups(paste: &PastePreview, eye: Vec3) -> Vec<MeshGroup> {
    let mut out = Vec::new();
    if !paste.active || paste.clipboard.is_empty() {
        return out;
    }
    let color = srgb_to_linear([0.0, 1.0, 0.6]);
    if paste.clipboard.len() <= MAX_ITEMS {
        let mut group = gizmo_group(0.7);
        for item in &paste.clipboard.blocks {
            let pos = transformed_cell(item.offset, paste.current_pivot, paste.yaw);
            cube(
                &mut group,
                pos.as_vec3() + Vec3::splat(0.5),
                Vec3::splat(0.52),
                color,
                eye,
            );
        }
        for item in &paste.clipboard.entities {
            let pos = transformed_cell(item.offset, paste.current_pivot, paste.yaw);
            cube(
                &mut group,
                pos.as_vec3() + Vec3::splat(0.5),
                Vec3::splat(0.65),
                color,
                eye,
            );
        }
        push_nonempty(&mut out, group);
    } else {
        let mut min = IVec3::splat(i32::MAX);
        let mut max = IVec3::splat(i32::MIN);
        for item in &paste.clipboard.blocks {
            let pos = transformed_cell(item.offset, paste.current_pivot, paste.yaw);
            min = min.min(pos);
            max = max.max(pos);
        }
        for item in &paste.clipboard.entities {
            let pos = transformed_cell(item.offset, paste.current_pivot, paste.yaw);
            min = min.min(pos);
            max = max.max(pos);
        }
        if min.x <= max.x {
            let lo = min.as_vec3();
            let hi = (max + IVec3::ONE).as_vec3();
            let mut group = gizmo_group(0.7);
            cube(&mut group, (lo + hi) * 0.5, (hi - lo) * 0.5, color, eye);
            push_nonempty(&mut out, group);
        }
    }
    out
}

fn track_groups(level: &LevelDocument, active: &ActiveTrack, eye: Vec3) -> Vec<MeshGroup> {
    let mut out = Vec::new();
    for track in &level.data.tracks {
        let pts = track.world_points();
        if pts.is_empty() {
            continue;
        }
        let is_active = active.0 == Some(track.id);
        let color = if is_active {
            srgb_to_linear([1.0, 0.9, 0.2])
        } else {
            srgb_to_linear([0.9, 0.55, 0.25])
        };
        let mut group = gizmo_group(if is_active { 1.0 } else { 0.8 });
        for w in pts.windows(2) {
            ribbon(&mut group, w[0], w[1], color, 0.04);
        }
        if track.mode == TrackMode::Loop && pts.len() > 2 {
            ribbon(&mut group, *pts.last().unwrap(), pts[0], color, 0.04);
        }
        for p in &pts {
            cube(&mut group, *p, Vec3::splat(0.12), color, eye);
        }
        push_nonempty(&mut out, group);
    }
    out
}

/// Edit-mode wires between same-channel linked entities, at the original's
/// 1.2-cell hover height (`entities_runtime::draw_link_gizmos`).
fn link_groups(level: &LevelDocument) -> Vec<MeshGroup> {
    let linked: Vec<&EntityData> = level
        .data
        .entities
        .iter()
        .filter(|entity| entity.link != 0 && entity.kind.uses_link())
        .collect();
    let mut out = Vec::new();
    for (index, a) in linked.iter().enumerate() {
        for b in &linked[index + 1..] {
            if a.link != b.link {
                continue;
            }
            let color = srgb_to_linear(link_color(a.link));
            let mut group = gizmo_group(0.8);
            let pa = a.cell_i().as_vec3() + Vec3::new(0.5, 1.2, 0.5);
            let pb = b.cell_i().as_vec3() + Vec3::new(0.5, 1.2, 0.5);
            ribbon(&mut group, pa, pb, color, 0.03);
            push_nonempty(&mut out, group);
        }
    }
    out
}

pub fn edit_groups(
    level: &LevelDocument,
    cursor_place: Option<IVec3>,
    tab: BrushTab,
    box_start: &BoxFillStart,
    selection: &SelectionSet,
    selected: &SelectedEntity,
    paste: &PastePreview,
    active: &ActiveTrack,
    eye: Vec3,
) -> Vec<MeshGroup> {
    let mut out = Vec::new();
    if tab == BrushTab::Blocks
        && let Some(a) = box_start.start
        && let Some(b) = cursor_place
    {
        out.extend(box_fill_groups(a, b, eye));
    }
    out.extend(selection_groups(level, selection, selected, eye));
    out.extend(paste_groups(paste, eye));
    out.extend(track_groups(level, active, eye));
    out.extend(link_groups(level));
    out
}
