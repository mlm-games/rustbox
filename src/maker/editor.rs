use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::block::{ALL_BLOCK_SHAPES, BlockKind, BlockShape};
use super::camera::WorldCamera;
use super::collision::raycast_present;
use super::commands::{CommandHistory, EditCommand};
use super::entity_data::{EntityData, EntityDataExt, EntityKind};
use super::level::{BlockData, LevelDocument};
use super::limits;
use super::mode::{
    ActiveLinkChannel, BlockBrush, BlockPlaced, BoxFillStart, BrushTab, ClipboardBlock,
    ClipboardEntity, EditorClipboard, EditorCursor, InputCapture, MakerMode, MirrorMode,
    PastePreview, PlaceYaw, SelectedEntity, SelectedEntityKind, SelectionBoxStart, SelectionSet,
};
use super::rendering::{MakerAssets, PlacementPreview, spawn_place_ghost};
use super::track::{ActiveTrack, TrackData, TrackMode};
use super::ui_bridge::MakerUi;

pub fn toggle_mode(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    mut mode: ResMut<MakerMode>,
) {
    if capture.ui_wants_keyboard {
        return;
    }
    if keys.just_pressed(KeyCode::Tab) {
        *mode = match *mode {
            MakerMode::Edit => MakerMode::Play,
            MakerMode::Play => MakerMode::Edit,
        };
    }
}

pub fn block_palette_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    tab: Res<BrushTab>,
    mut brush: ResMut<BlockBrush>,
    mut ui: ResMut<MakerUi>,
) {
    if capture.ui_wants_keyboard {
        return;
    }
    if *tab != BrushTab::Blocks {
        return;
    }
    if keys.just_pressed(KeyCode::Digit1) {
        brush.kind = BlockKind::Grass;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        brush.kind = BlockKind::Stone;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        brush.kind = BlockKind::Hazard;
    }
    if keys.just_pressed(KeyCode::Digit4) {
        brush.kind = BlockKind::Goal;
    }
    if keys.just_pressed(KeyCode::Digit5) {
        brush.kind = BlockKind::Spawn;
    }
    if keys.just_pressed(KeyCode::Digit6) {
        brush.kind = BlockKind::Water;
        ui.set_status("Water: fills a cell with swimmable water");
    }
    if keys.just_pressed(KeyCode::Digit7) {
        brush.kind = BlockKind::Ice;
        ui.set_status("Ice: slippery surface");
    }
    if keys.just_pressed(KeyCode::Digit8) {
        brush.kind = BlockKind::Spikes;
        ui.set_status("Spikes: floor/ceiling hazard");
    }
    if keys.just_pressed(KeyCode::Digit9) {
        brush.kind = BlockKind::Conveyor;
        ui.set_status("Conveyor: pushes along its facing (R rotates)");
    }
    if keys.just_pressed(KeyCode::Digit0) {
        brush.kind = BlockKind::Bounce;
        ui.set_status("Bounce: springs you up");
    }
    if keys.just_pressed(KeyCode::KeyR) {
        brush.rot = (brush.rot.wrapping_add(1)) % 4;
        ui.set_status(format!("Block rotation: {}°", (brush.rot as u16) * 90));
    }
    if keys.just_pressed(KeyCode::KeyT) {
        let shapes = ALL_BLOCK_SHAPES;
        let idx = shapes.iter().position(|s| *s == brush.shape).unwrap_or(0);
        brush.shape = shapes[(idx + 1) % shapes.len()];
        ui.set_status(format!("Block shape: {}", brush.shape.name()));
    }
    if keys.just_pressed(KeyCode::KeyY) {
        brush.kind = BlockKind::OneWay;
        ui.set_status("One-Way: land on top, pass through from below/sides");
    }
    if keys.just_pressed(KeyCode::KeyP) {
        brush.kind = BlockKind::TimedPulse;
        ui.set_status("Timed Pulse: solid while on/off channel is ON");
    }
    if keys.just_pressed(KeyCode::KeyU) {
        brush.waterlogged = !brush.waterlogged;
        ui.set_status(if brush.waterlogged {
            "Waterlogged: on (blocks fill their cell with water)"
        } else {
            "Waterlogged: off"
        });
    }
}

pub fn entity_palette_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    mut tab: ResMut<BrushTab>,
    mut sel_e: ResMut<SelectedEntityKind>,
    mut place_yaw: ResMut<PlaceYaw>,
    mut channel: ResMut<ActiveLinkChannel>,
    mut ui: ResMut<MakerUi>,
) {
    if capture.ui_wants_keyboard {
        return;
    }
    if keys.just_pressed(KeyCode::KeyQ) {
        *tab = match *tab {
            BrushTab::Blocks => BrushTab::Entities,
            BrushTab::Entities => BrushTab::Tracks,
            BrushTab::Tracks => BrushTab::Blocks,
        };
    }
    if keys.just_pressed(KeyCode::KeyF) {
        if shift_pressed(&keys) {
            return;
        }
        place_yaw.0 = (place_yaw.0 + 45.0) % 360.0;
    }
    if keys.just_pressed(KeyCode::KeyL) {
        channel.0 = channel.0 % 9 + 1;
        ui.set_status(format!("Link channel: {}", channel.0));
    }
    if *tab != BrushTab::Entities {
        return;
    }
    if keys.just_pressed(KeyCode::Digit1) {
        sel_e.0 = EntityKind::Glimmer;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        sel_e.0 = EntityKind::LaunchPad;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        sel_e.0 = EntityKind::Seal;
    }
    if keys.just_pressed(KeyCode::Digit4) {
        sel_e.0 = EntityKind::DriftPlate;
    }
    if keys.just_pressed(KeyCode::Digit5) {
        sel_e.0 = EntityKind::Prowler;
    }
    if keys.just_pressed(KeyCode::Digit6) {
        sel_e.0 = EntityKind::TriggerOrb;
    }
    if keys.just_pressed(KeyCode::Digit7) {
        sel_e.0 = EntityKind::RelayGate;
    }
    if keys.just_pressed(KeyCode::Digit8) {
        sel_e.0 = EntityKind::Teleporter;
    }
    if keys.just_pressed(KeyCode::Digit9) {
        sel_e.0 = EntityKind::Fan;
    }
    if keys.just_pressed(KeyCode::Digit0) {
        sel_e.0 = EntityKind::Bumper;
    }
}

pub fn track_tool_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    tab: Res<BrushTab>,
    mut active: ResMut<ActiveTrack>,
    mut history: ResMut<CommandHistory>,
    mut level: ResMut<LevelDocument>,
) {
    if capture.ui_wants_keyboard {
        return;
    }
    if *tab != BrushTab::Tracks {
        return;
    }
    let Some(id) = active.0 else {
        return;
    };
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Escape) {
        active.0 = None;
        return;
    }
    let (mode, speed) = level
        .track(id)
        .map(|t| (t.mode, t.speed))
        .unwrap_or((TrackMode::PingPong, default_speed()));
    if keys.just_pressed(KeyCode::KeyM) {
        let new = match mode {
            TrackMode::PingPong => TrackMode::Loop,
            TrackMode::Loop => TrackMode::PingPong,
        };
        history.apply(
            &mut level,
            EditCommand::SetTrackMode {
                track_id: id,
                old: mode,
                new,
            },
        );
    }
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        history.apply(
            &mut level,
            EditCommand::SetTrackSpeed {
                track_id: id,
                old: speed,
                new: (speed + 0.5).min(10.0),
            },
        );
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        history.apply(
            &mut level,
            EditCommand::SetTrackSpeed {
                track_id: id,
                old: speed,
                new: (speed - 0.5).max(0.5),
            },
        );
    }
}

fn default_speed() -> f32 {
    2.0
}

pub fn mirror_hotkey(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    mut mirror: ResMut<MirrorMode>,
    mut ui: ResMut<MakerUi>,
) {
    if capture.ui_wants_keyboard {
        return;
    }
    if !ctrl_pressed(&keys) && keys.just_pressed(KeyCode::KeyV) {
        mirror.0 = (mirror.0 + 1) % 4;
        let label = match mirror.0 {
            0 => "Off",
            1 => "X",
            2 => "Z",
            _ => "X+Z",
        };
        ui.set_status(format!("Mirror: {label}"));
    }
}

/// Cells affected by a placement at `cell` under the given mirror mode
/// (bit 0 = X mirror, bit 1 = Z mirror).
fn mirror_cells(cell: IVec3, mode: u8) -> Vec<IVec3> {
    let mut out = vec![cell];
    if mode & 1 != 0 {
        out.push(IVec3::new(-cell.x, cell.y, cell.z));
    }
    if mode & 2 != 0 {
        out.push(IVec3::new(cell.x, cell.y, -cell.z));
    }
    if mode & 3 == 3 {
        out.push(IVec3::new(-cell.x, cell.y, -cell.z));
    }
    out.sort_by_key(|c| (c.x, c.y, c.z));
    out.dedup();
    out
}

/// Minimum screen-space movement (pixels squared) before hold-drag may
/// place/erase another cell. Blocks raycast extrusion while the pointer stays.
const DRAG_POINTER_EPSILON_SQ: f32 = 1.0; // 1px

fn pointer_moved_since(last: Option<Vec2>, now: Option<Vec2>) -> bool {
    match (last, now) {
        (Some(a), Some(b)) => a.distance_squared(b) > DRAG_POINTER_EPSILON_SQ,
        // No anchor yet → not a continuation of a drag stroke.
        _ => false,
    }
}

fn build_block_data(
    kind: BlockKind,
    shape: BlockShape,
    rot: u8,
    waterlogged: bool,
    cell: IVec3,
) -> BlockData {
    // Water is always a full, unlogged cell fill.
    let shape = if kind == BlockKind::Water {
        BlockShape::Full
    } else if kind.is_thin() {
        BlockShape::Thin
    } else {
        shape
    };
    let waterlogged = if kind == BlockKind::Water {
        false
    } else {
        waterlogged
    };
    BlockData {
        position: cell.to_array(),
        kind,
        shape,
        rot,
        waterlogged,
    }
}

fn ctrl_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
}

fn shift_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
}

fn selection_anchor_cell(cursor: &EditorCursor) -> Option<IVec3> {
    cursor.hit.or(cursor.place)
}

fn cell_in_aabb(cell: IVec3, min: IVec3, max: IVec3) -> bool {
    cell.x >= min.x
        && cell.x <= max.x
        && cell.y >= min.y
        && cell.y <= max.y
        && cell.z >= min.z
        && cell.z <= max.z
}

fn selection_pivot(level: &LevelDocument, selection: &SelectionSet) -> Option<IVec3> {
    let mut pivot: Option<IVec3> = None;

    for &cell in &selection.blocks {
        pivot = Some(match pivot {
            Some(p) => IVec3::new(p.x.min(cell.x), p.y.min(cell.y), p.z.min(cell.z)),
            None => cell,
        });
    }

    for &id in &selection.entities {
        if let Some(entity) = level.entity_by_id(id) {
            let cell = entity.cell_i();
            pivot = Some(match pivot {
                Some(p) => IVec3::new(p.x.min(cell.x), p.y.min(cell.y), p.z.min(cell.z)),
                None => cell,
            });
        }
    }

    pivot
}

fn copy_selection_to_clipboard(
    level: &LevelDocument,
    selection: &SelectionSet,
    clipboard: &mut EditorClipboard,
) -> usize {
    clipboard.clear();

    let Some(pivot) = selection_pivot(level, selection) else {
        return 0;
    };

    for &cell in &selection.blocks {
        if let Some(block) = level.get_block(cell).cloned() {
            clipboard.blocks.push(ClipboardBlock {
                offset: cell - pivot,
                data: block,
            });
        }
    }

    for &id in &selection.entities {
        if let Some(entity) = level.entity_by_id(id).cloned() {
            clipboard.entities.push(ClipboardEntity {
                offset: entity.cell_i() - pivot,
                data: entity,
            });
        }
    }

    clipboard.len()
}

fn delete_selection(
    level: &mut LevelDocument,
    history: &mut CommandHistory,
    selection: &mut SelectionSet,
    selected_entity: &mut SelectedEntity,
) -> usize {
    let mut blocks = Vec::new();
    let mut entities = Vec::new();

    for &cell in &selection.blocks {
        if let Some(block) = level.get_block(cell).cloned() {
            blocks.push((cell, block));
        }
    }

    for &id in &selection.entities {
        if let Some(entity) = level.entity_by_id(id).cloned() {
            entities.push(entity);
        }
    }

    let count = blocks.len() + entities.len();
    if count == 0 {
        selection.clear();
        selected_entity.0 = None;
        return 0;
    }

    history.apply(level, EditCommand::DeleteSelection { blocks, entities });

    selection.clear();
    selected_entity.0 = None;
    count
}

fn rotate_clipboard_yaw(yaw: &mut f32) {
    *yaw = (*yaw + 90.0) % 360.0;
}

fn transformed_cell(offset: IVec3, pivot: IVec3, yaw: f32) -> IVec3 {
    let mut p = offset;
    match yaw as i32 % 360 {
        90 => p = IVec3::new(-p.z, p.y, p.x),
        180 => p = IVec3::new(-p.x, p.y, -p.z),
        270 => p = IVec3::new(p.z, p.y, -p.x),
        _ => {}
    }
    pivot + p
}

fn paste_clipboard(
    level: &mut LevelDocument,
    history: &mut CommandHistory,
    selection: &mut SelectionSet,
    selected_entity: &mut SelectedEntity,
    clipboard: &EditorClipboard,
    target: IVec3,
    yaw: f32,
) -> usize {
    if clipboard.is_empty() {
        return 0;
    }

    let mut blocks = Vec::new();
    let mut entities = Vec::new();

    for item in &clipboard.blocks {
        let pos = transformed_cell(item.offset, target, yaw);

        // Do not paste into invisible boundary solids.
        if level.boundary_solid(pos) {
            continue;
        }

        let mut data = item.data.clone();
        data.position = pos.to_array();
        data.rot = (data.rot + (yaw as u8 / 90) as u8) % 4;

        let previous = level.get_block(pos).cloned();
        blocks.push((pos, data, previous));
    }

    for item in &clipboard.entities {
        let pos = transformed_cell(item.offset, target, yaw);

        // Honor stacking rules when pasting into a cell.
        if !level.can_place_entity_at(pos, item.data.kind) {
            continue;
        }

        let mut entity = item.data.clone();
        let old_cell = entity.cell_i();

        entity.id = level.alloc_id();
        entity.cell = pos.to_array();
        entity.yaw_deg = (entity.yaw_deg + yaw) % 360.0;

        // Move legacy paired-cell data with the pasted entity.
        if let Some(cell_b) = entity.cell_b {
            let relative_b = IVec3::from_array(cell_b) - old_cell;
            let rotated_b = transformed_cell(relative_b, IVec3::ZERO, yaw);
            entity.cell_b = Some((pos + rotated_b).to_array());
        }

        entities.push(entity);
    }

    let count = blocks.len() + entities.len();
    if count == 0 {
        return 0;
    }

    let pasted_blocks: Vec<IVec3> = blocks.iter().map(|(pos, _, _)| *pos).collect();
    let pasted_entities: Vec<_> = entities.iter().map(|entity| entity.id).collect();

    history.apply(level, EditCommand::PasteSelection { blocks, entities });

    selection.clear();

    for cell in pasted_blocks {
        selection.blocks.insert(cell);
    }

    for id in pasted_entities {
        selection.entities.insert(id);
    }

    selected_entity.0 = selection.entities.iter().next().copied();

    count
}

/// Live paste preview controls (should have a help menu for shortcuts ig):
/// - R rotates the preview 90°
/// - Left-click commits the paste
/// - Right-click or Escape cancels
pub fn update_paste_preview(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    capture: Res<InputCapture>,
    cursor: Res<EditorCursor>,
    mut preview: ResMut<PastePreview>,
    mut selection: ResMut<SelectionSet>,
    mut selected_entity: ResMut<SelectedEntity>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
    mut ui: ResMut<MakerUi>,
    mut box_select: ResMut<SelectionBoxStart>,
) {
    if !preview.active {
        return;
    }

    if capture.ui_wants_keyboard {
        return;
    }

    if capture.ui_wants_pointer {
        return;
    }

    if keys.just_pressed(KeyCode::KeyR) {
        rotate_clipboard_yaw(&mut preview.yaw);
        ui.set_status(format!("Preview rotated to {}°", preview.yaw));
        return;
    }

    if keys.just_pressed(KeyCode::Escape) || buttons.just_pressed(MouseButton::Right) {
        preview.reset();
        box_select.start = None;
        ui.set_status("Paste preview cancelled");
        return;
    }

    // Update preview target to current cursor.
    if let Some(target) = cursor.place.or(cursor.hit) {
        preview.current_pivot = target;
    }

    // Commit on left click. The preview stays "active" until the next frame so
    // normal block placement is suppressed for this commit click.
    if buttons.just_pressed(MouseButton::Left) {
        let count = paste_clipboard(
            &mut level,
            &mut history,
            &mut selection,
            &mut selected_entity,
            &preview.clipboard,
            preview.current_pivot,
            preview.yaw,
        );

        ui.set_status(format!("Pasted {count} item(s)"));
        preview.active = false;
    }
}

pub fn draw_box_fill_preview(
    mode: Res<MakerMode>,
    tab: Res<BrushTab>,
    cursor: Res<EditorCursor>,
    box_start: Res<BoxFillStart>,
    mut gizmos: Gizmos,
) {
    if *mode != MakerMode::Edit || *tab != BrushTab::Blocks {
        return;
    }
    let Some(a) = box_start.start else {
        return;
    };
    let Some(b) = cursor.place else {
        return;
    };

    let color = Color::srgb(0.95, 0.9, 0.2);
    let min = a.min(b);
    let max = a.max(b);

    // World bounds covering every cell from `min` to `max` inclusive.
    let lo = min.as_vec3();
    let hi = (max + IVec3::ONE).as_vec3();
    let center = (lo + hi) * 0.5;
    let size = hi - lo;

    // Outlined edges to mark the box bounds.
    draw_aabb(&mut gizmos, center, size * 0.5, color);

    // Grid lines across each face so the fill extent reads as a region.
    let steps = (size / 1.0).floor().min(Vec3::splat(48.0));
    let sx = steps.x as i32;
    let sz = steps.z as i32;
    if sx > 1 {
        for i in 1..sx {
            let t = i as f32 / sx as f32;
            let x = lo.x + size.x * t;
            gizmos.line(
                Vec3::new(x, lo.y, lo.z),
                Vec3::new(x, lo.y, hi.z),
                color.with_alpha(0.25),
            );
        }
    }
    if sz > 1 {
        for i in 1..sz {
            let t = i as f32 / sz as f32;
            let z = lo.z + size.z * t;
            gizmos.line(
                Vec3::new(lo.x, lo.y, z),
                Vec3::new(hi.x, lo.y, z),
                color.with_alpha(0.25),
            );
        }
    }

    // Corner markers.
    for c in [
        lo,
        Vec3::new(hi.x, lo.y, lo.z),
        Vec3::new(lo.x, lo.y, hi.z),
        hi,
    ] {
        gizmos.sphere(Isometry3d::from_translation(c), 0.08, color);
    }
}

pub fn draw_paste_preview_gizmos(preview: Res<PastePreview>, mut gizmos: Gizmos) {
    if !preview.active || preview.clipboard.is_empty() {
        return;
    }

    let preview_color = Color::srgb(0.0, 1.0, 0.6);
    let alpha = 0.7;

    for item in &preview.clipboard.blocks {
        let world_pos = transformed_cell(item.offset, preview.current_pivot, preview.yaw);
        let center = world_pos.as_vec3() + Vec3::splat(0.5);
        draw_aabb(
            &mut gizmos,
            center,
            Vec3::splat(0.52),
            preview_color.with_alpha(alpha),
        );
    }

    for item in &preview.clipboard.entities {
        let world_pos = transformed_cell(item.offset, preview.current_pivot, preview.yaw);
        let center = world_pos.as_vec3() + Vec3::new(0.5, 0.5, 0.5);
        draw_aabb(
            &mut gizmos,
            center,
            Vec3::splat(0.65),
            preview_color.with_alpha(alpha),
        );
    }
}

/// Structure-editing hotkeys:
/// - Ctrl+LeftClick: toggle block/entity under cursor
/// - B, then B again: select volume corners
/// - Ctrl+A: select all editable blocks/entities
/// - Ctrl+C: copy selection
/// - Ctrl+X: cut selection
/// - Ctrl+V: paste at cursor
/// - Delete: delete selection
/// - Escape: clear selection
pub fn selection_hotkeys(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    cursor: Res<EditorCursor>,
    mut selection: ResMut<SelectionSet>,
    mut box_select: ResMut<SelectionBoxStart>,
    mut clipboard: ResMut<EditorClipboard>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
    mut selected_entity: ResMut<SelectedEntity>,
    mut ui: ResMut<MakerUi>,
    mut preview: ResMut<PastePreview>,
) {
    if capture.ui_wants_keyboard {
        return;
    }

    if preview.active {
        return;
    }

    let ctrl = ctrl_pressed(&keys);
    let shift = shift_pressed(&keys);

    if keys.just_pressed(KeyCode::Escape) {
        selection.clear();
        box_select.start = None;
        selected_entity.0 = None;
        ui.set_status("Selection cleared");
        return;
    }

    if ctrl && buttons.just_pressed(MouseButton::Left) {
        let Some(cell) = selection_anchor_cell(&cursor) else {
            return;
        };

        if !shift {
            // Ctrl+Shift+Click can add/remove without clearing,
            // Ctrl+Click starts a fresh targeted selection.
            selection.clear();
        }

        if let Some(entity) = level.top_entity_at_cell(cell) {
            selection.toggle_entity(entity.id);
            selected_entity.0 = Some(entity.id);
        } else if level.get_block(cell).is_some() {
            selection.toggle_block(cell);
            selected_entity.0 = None;
        }

        ui.set_status(format!("Selected {} item(s)", selection.len()));
        return;
    }

    if keys.just_pressed(KeyCode::KeyB) {
        let Some(cell) = selection_anchor_cell(&cursor) else {
            ui.set_status("Aim at the level to volume-select");
            return;
        };

        match box_select.start {
            None => {
                box_select.start = Some(cell);
                ui.set_status("Selection corner set. Press B on the opposite corner.");
            }
            Some(start) => {
                box_select.start = None;

                if !shift {
                    selection.clear();
                    selected_entity.0 = None;
                }

                let min = start.min(cell);
                let max = start.max(cell);

                for &block_cell in level.map.keys() {
                    if cell_in_aabb(block_cell, min, max) {
                        selection.blocks.insert(block_cell);
                    }
                }

                for entity in &level.data.entities {
                    if cell_in_aabb(entity.cell_i(), min, max) {
                        selection.entities.insert(entity.id);
                    }
                }

                selected_entity.0 = selection.entities.iter().next().copied();
                ui.set_status(format!("Volume selected {} item(s)", selection.len()));
            }
        }

        return;
    }

    if ctrl && keys.just_pressed(KeyCode::KeyA) {
        selection.clear();
        selection.blocks.extend(level.map.keys().copied());
        selection
            .entities
            .extend(level.data.entities.iter().map(|e| e.id));
        selected_entity.0 = selection.entities.iter().next().copied();
        ui.set_status(format!("Selected all {} item(s)", selection.len()));
        return;
    }

    if ctrl && keys.just_pressed(KeyCode::KeyC) {
        let count = copy_selection_to_clipboard(&level, &selection, &mut clipboard);
        ui.set_status(if count == 0 {
            "Nothing selected to copy".to_string()
        } else {
            format!("Copied {count} item(s)")
        });
        return;
    }

    if ctrl && keys.just_pressed(KeyCode::KeyX) {
        let copied = copy_selection_to_clipboard(&level, &selection, &mut clipboard);
        if copied == 0 {
            ui.set_status("Nothing selected to cut");
            return;
        }

        let deleted = delete_selection(
            &mut level,
            &mut history,
            &mut selection,
            &mut selected_entity,
        );
        ui.set_status(format!("Cut {deleted} item(s)"));
        return;
    }

    if ctrl && keys.just_pressed(KeyCode::KeyV) {
        if clipboard.is_empty() {
            ui.set_status("Clipboard is empty");
            return;
        }

        let Some(target) = cursor.place.or(cursor.hit) else {
            ui.set_status("Aim at the level to start paste preview");
            return;
        };

        preview.active = true;
        preview.clipboard = clipboard.clone();
        preview.current_pivot = target;
        preview.yaw = 0.0;

        ui.set_status(
            "Paste preview active - Left click to place, R to rotate, Right click/Escape to cancel",
        );
        return;
    }

    if keys.just_pressed(KeyCode::Delete) && !selection.is_empty() {
        let count = delete_selection(
            &mut level,
            &mut history,
            &mut selection,
            &mut selected_entity,
        );
        ui.set_status(format!("Deleted {count} item(s)"));
    }
}

pub fn undo_redo_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    capture: Res<InputCapture>,
    mut history: ResMut<CommandHistory>,
    mut level: ResMut<LevelDocument>,
) {
    if capture.ui_wants_keyboard {
        return;
    }
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if ctrl && keys.just_pressed(KeyCode::KeyZ) {
        history.undo(&mut level);
    }
    if ctrl && keys.just_pressed(KeyCode::KeyY) {
        history.redo(&mut level);
    }
}

/// Recomputes the cell the edit cursor points at, so the heavier edit system
/// can stay under Bevy's 16-system-param limit.
pub fn update_editor_cursor(
    capture: Res<InputCapture>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam_q: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    level: Res<LevelDocument>,
    mut cursor: ResMut<EditorCursor>,
) {
    if capture.ui_wants_pointer {
        *cursor = EditorCursor::default();
        return;
    }
    let Ok(window) = windows.single() else {
        *cursor = EditorCursor::default();
        return;
    };
    let Some(pos) = window.cursor_position() else {
        *cursor = EditorCursor::default();
        return;
    };
    let Ok((camera, cam_tf)) = cam_q.single() else {
        *cursor = EditorCursor::default();
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_tf, pos) else {
        *cursor = EditorCursor::default();
        return;
    };
    cursor.pointer = Some(pos);
    cursor.hit = None;
    cursor.place = None;
    if let Some((hit, normal)) = raycast_present(&level, ray.origin, *ray.direction, 200.0) {
        cursor.hit = Some(hit);
        cursor.place = Some(hit + normal);
    }
}

/// Moves the placement ghost to the cursor's target cell (Blocks tab) and
/// updates its mesh, material + rotation to match the selected block shape,
/// kind color and rot.
pub fn update_placement_preview(
    cursor: Res<EditorCursor>,
    tab: Res<BrushTab>,
    brush: Res<BlockBrush>,
    assets: Option<Res<MakerAssets>>,
    mut preview_q: Query<
        (
            &mut Transform,
            &mut Visibility,
            &mut Mesh3d,
            &mut MeshMaterial3d<StandardMaterial>,
        ),
        With<PlacementPreview>,
    >,
) {
    let Ok((mut tr, mut vis, mut mesh, mut mat)) = preview_q.single_mut() else {
        return;
    };
    if *tab == BrushTab::Tracks {
        *vis = Visibility::Hidden;
        return;
    }
    let Some(place_cell) = cursor.place else {
        *vis = Visibility::Hidden;
        return;
    };
    if *tab == BrushTab::Entities {
        *vis = Visibility::Hidden;
        return;
    }
    let Some(assets) = assets else {
        *vis = Visibility::Hidden;
        return;
    };
    if let Some(handle) = assets.shape_meshes.get(&brush.shape) {
        *mesh = Mesh3d(handle.clone());
    }
    if let Some(handle) = assets.ghost_alpha_mats.get(&brush.kind) {
        *mat = MeshMaterial3d(handle.clone());
    }
    tr.translation = Vec3::new(
        place_cell.x as f32 + 0.5,
        place_cell.y as f32 + 0.5,
        place_cell.z as f32 + 0.5,
    );
    tr.rotation = Quat::from_rotation_y(brush.rot as f32 * std::f32::consts::FRAC_PI_2);
    tr.scale = Vec3::splat(1.02);
    *vis = Visibility::Visible;
}

pub fn update_preview_and_edit(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    cursor: Res<EditorCursor>,
    mut brush: ResMut<BlockBrush>,
    mut sel_e: ResMut<SelectedEntityKind>,
    mut tab: ResMut<BrushTab>,
    place_yaw: Res<PlaceYaw>,
    mirror: Res<MirrorMode>,
    mut box_start: ResMut<BoxFillStart>,
    mut active: ResMut<ActiveTrack>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
    limits: Res<limits::LevelLimits>,
    mut sel_ent: ResMut<SelectedEntity>,
    channel: Res<ActiveLinkChannel>,
    mut placed: MessageWriter<BlockPlaced>,
) {
    let Some(hit_cell) = cursor.hit else {
        return;
    };
    let Some(place_cell) = cursor.place else {
        return;
    };

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let pointer = cursor.pointer;
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if ctrl {
        return;
    }

    // End stroke when the button is released so the next click is a fresh
    // single place.
    if !buttons.pressed(MouseButton::Left) {
        box_start.last_paint = None;
    }
    if !buttons.pressed(MouseButton::Right) {
        box_start.last_erase = None;
    }
    if !buttons.pressed(MouseButton::Left) && !buttons.pressed(MouseButton::Right) {
        box_start.last_pointer = None;
    }

    // Eyedropper: middle-click picks the block/entity under the cursor.
    if buttons.just_pressed(MouseButton::Middle) {
        if let Some(b) = level.get_block(hit_cell) {
            brush.kind = b.kind;
            brush.shape = b.shape;
            brush.rot = b.rot % 4;
            brush.waterlogged = b.waterlogged;
            *tab = BrushTab::Blocks;
        } else if let Some(ent) = level.top_entity_at_cell(hit_cell) {
            sel_e.0 = ent.kind;
            *tab = BrushTab::Entities;
        }
    }

    if buttons.just_pressed(MouseButton::Left) {
        match *tab {
            BrushTab::Blocks => {
                if shift {
                    match box_start.start {
                        None => box_start.start = Some(place_cell),
                        Some(a) => {
                            box_start.start = None;
                            let min = a.min(place_cell);
                            let max = a.max(place_cell);
                            let mut cells = Vec::new();
                            for x in min.x..=max.x {
                                for y in min.y..=max.y {
                                    for z in min.z..=max.z {
                                        let c = IVec3::new(x, y, z);
                                        let prev = level.get_block(c).cloned();
                                        let same = prev.as_ref().is_some_and(|b| {
                                            (b.kind, b.shape, b.rot, b.waterlogged)
                                                == (
                                                    brush.kind,
                                                    brush.shape,
                                                    brush.rot,
                                                    brush.waterlogged,
                                                )
                                        });
                                        if !same {
                                            cells.push((c, prev));
                                        }
                                    }
                                }
                            }
                            if !cells.is_empty() && (level.map.len() as u32) < limits.max_blocks {
                                history.apply(
                                    &mut level,
                                    EditCommand::BoxFill {
                                        cells,
                                        data: build_block_data(
                                            brush.kind,
                                            brush.shape,
                                            brush.rot,
                                            brush.waterlogged,
                                            place_cell,
                                        ),
                                    },
                                );
                            }
                        }
                    }
                } else {
                    for cell in mirror_cells(place_cell, mirror.0) {
                        if level.get_block(cell).is_none()
                            && !level.boundary_solid(cell)
                            && (level.map.len() as u32) < limits.max_blocks
                        {
                            history.apply(
                                &mut level,
                                EditCommand::Place {
                                    position: cell,
                                    data: build_block_data(
                                        brush.kind,
                                        brush.shape,
                                        brush.rot,
                                        brush.waterlogged,
                                        cell,
                                    ),
                                    previous: None,
                                },
                            );
                            placed.write(BlockPlaced {
                                cell,
                                kind: brush.kind,
                                shape: if brush.kind == BlockKind::Water {
                                    BlockShape::Full
                                } else {
                                    brush.shape
                                },
                                rot: if brush.kind == BlockKind::Water {
                                    0
                                } else {
                                    brush.rot
                                },
                            });
                        }
                    }
                    // Anchor stroke: one block this click; drag needs pointer motion.
                    box_start.last_paint = Some(place_cell);
                    box_start.last_pointer = pointer;
                }
            }
            BrushTab::Entities => {
                if (level.data.entities.len() as u32) < limits.max_entities
                    && level.can_place_entity_at(place_cell, sel_e.0)
                {
                    let id = level.alloc_id();
                    let mut data = EntityData::defaults_for(sel_e.0, place_cell, id);
                    data.yaw_deg = place_yaw.0;
                    if data.kind.uses_link() {
                        data.link = channel.0;
                    }
                    if data.kind == EntityKind::Cannon {
                        let yaw = place_yaw.0.to_radians();
                        let dir = IVec3::new(
                            (yaw.sin() * 4.0).round() as i32,
                            0,
                            (yaw.cos() * 4.0).round() as i32,
                        );
                        data.cell_b = Some((place_cell + dir).to_array());
                    }
                    let world = place_cell.as_vec3() + Vec3::new(0.5, 0.0, 0.5);
                    data.track = level.track_near(world, 1.5);
                    history.apply(&mut level, EditCommand::PlaceEntity { entity: data });
                }
            }
            BrushTab::Tracks => {
                if let Some(id) = level.track_at_cell(place_cell) {
                    active.0 = Some(id);
                } else if let Some(id) = active.0 {
                    let last = level.track(id).and_then(|t| t.points.last().copied());
                    if last != Some(place_cell.to_array()) {
                        let index = level.track(id).map(|t| t.points.len()).unwrap_or(0);
                        history.apply(
                            &mut level,
                            EditCommand::AddTrackPoint {
                                track_id: id,
                                index,
                                cell: place_cell.to_array(),
                            },
                        );
                    }
                } else if (level.data.tracks.len() as u32) < limits.max_tracks {
                    let id = level.alloc_track_id();
                    let track = TrackData {
                        id,
                        points: vec![place_cell.to_array()],
                        mode: TrackMode::default(),
                        speed: default_speed(),
                    };
                    history.apply(&mut level, EditCommand::CreateTrack { track });
                    active.0 = Some(id);
                }
            }
        }
    } else if buttons.just_pressed(MouseButton::Right) {
        if *tab == BrushTab::Tracks {
            if let Some(id) = active.0 {
                let on_waypoint = level.track_at_cell(place_cell) == Some(id);
                let (index, len) = if on_waypoint {
                    match level.track(id) {
                        Some(t) => match t
                            .points
                            .iter()
                            .position(|p| IVec3::from_array(*p) == place_cell)
                        {
                            Some(i) => (i, t.points.len()),
                            None => (0, 0),
                        },
                        None => (0, 0),
                    }
                } else {
                    let len = level.track(id).map(|t| t.points.len()).unwrap_or(0);
                    if len > 0 { (len - 1, len) } else { (0, 0) }
                };
                if len > 0 {
                    if len <= 1 {
                        if let Some(track) = level.track(id).cloned() {
                            history.apply(&mut level, EditCommand::DeleteTrack { track });
                        }
                        active.0 = None;
                    } else {
                        let cell = level.track(id).map(|t| t.points[index]).unwrap();
                        history.apply(
                            &mut level,
                            EditCommand::RemoveTrackPoint {
                                track_id: id,
                                index,
                                cell,
                            },
                        );
                    }
                }
            } else if let Some(id) = level.track_at_cell(place_cell) {
                active.0 = Some(id);
            }
        } else if level.get_block(hit_cell).is_some() {
            for cell in mirror_cells(hit_cell, mirror.0) {
                if let Some(k) = level.get_block(cell).cloned() {
                    history.apply(
                        &mut level,
                        EditCommand::Remove {
                            position: cell,
                            previous: k,
                        },
                    );
                }
            }
            // Anchor erase stroke (prevents tunneling while held still).
            box_start.last_erase = Some(hit_cell);
            box_start.last_pointer = pointer;
        } else if let Some(ent) = level.top_entity_at_cell(hit_cell).cloned() {
            let removed_id = ent.id;
            history.apply(&mut level, EditCommand::RemoveEntity { entity: ent });
            if sel_ent.0 == Some(removed_id) {
                sel_ent.0 = None;
            }
        }
    }

    // Drag paint/erase: only when the pointer actually moved since last click.
    if *tab == BrushTab::Blocks && !shift {
        let drag_ok = pointer_moved_since(box_start.last_pointer, pointer);

        if buttons.pressed(MouseButton::Left)
            && !buttons.just_pressed(MouseButton::Left)
            && drag_ok
            && box_start.last_paint != Some(place_cell)
        {
            for cell in mirror_cells(place_cell, mirror.0) {
                if level.get_block(cell).is_none()
                    && !level.boundary_solid(cell)
                    && (level.map.len() as u32) < limits.max_blocks
                {
                    history.apply(
                        &mut level,
                        EditCommand::Place {
                            position: cell,
                            data: build_block_data(
                                brush.kind,
                                brush.shape,
                                brush.rot,
                                brush.waterlogged,
                                cell,
                            ),
                            previous: None,
                        },
                    );
                    placed.write(BlockPlaced {
                        cell,
                        kind: brush.kind,
                        shape: if brush.kind == BlockKind::Water {
                            BlockShape::Full
                        } else {
                            brush.shape
                        },
                        rot: if brush.kind == BlockKind::Water {
                            0
                        } else {
                            brush.rot
                        },
                    });
                }
            }
            box_start.last_paint = Some(place_cell);
            box_start.last_pointer = pointer;
        } else if buttons.pressed(MouseButton::Right)
            && !buttons.just_pressed(MouseButton::Right)
            && drag_ok
            && box_start.last_erase != Some(hit_cell)
        {
            for cell in mirror_cells(hit_cell, mirror.0) {
                if let Some(k) = level.get_block(cell).cloned() {
                    history.apply(
                        &mut level,
                        EditCommand::Remove {
                            position: cell,
                            previous: k,
                        },
                    );
                }
            }
            box_start.last_erase = Some(hit_cell);
            box_start.last_pointer = pointer;
        }
    }
}

pub fn spawn_place_ghosts(
    mut placed: MessageReader<BlockPlaced>,
    assets: Option<Res<MakerAssets>>,
    mut commands: Commands,
) {
    let Some(assets) = assets else {
        return;
    };
    for ev in placed.read() {
        spawn_place_ghost(&mut commands, &assets, ev.cell, ev.kind, ev.shape, ev.rot);
    }
}

pub fn delete_selected_entity(
    keys: Res<ButtonInput<KeyCode>>,
    mode: Res<MakerMode>,
    mut sel_ent: ResMut<SelectedEntity>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
) {
    if *mode != MakerMode::Edit {
        return;
    }
    if !keys.just_pressed(KeyCode::Delete) {
        return;
    }
    let Some(id) = sel_ent.0 else {
        return;
    };
    if let Some(entity) = level.entity_by_id(id).cloned() {
        history.apply(&mut level, EditCommand::RemoveEntity { entity });
        sel_ent.0 = None;
    }
}

pub fn draw_selected_entity_gizmo(
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    sel_ent: Res<SelectedEntity>,
    mut gizmos: Gizmos,
) {
    if *mode != MakerMode::Edit {
        return;
    }
    let Some(id) = sel_ent.0 else {
        return;
    };
    let Some(data) = level.entity_by_id(id) else {
        return;
    };
    let center = data.cell_i().as_vec3() + Vec3::new(0.5, 0.0, 0.5);
    let half = match data.kind {
        EntityKind::Glimmer => Vec3::splat(0.45),
        EntityKind::LaunchPad => Vec3::new(0.55, 0.3, 0.55),
        EntityKind::Seal => Vec3::new(0.6, 1.1, 0.4),
        EntityKind::DriftPlate => Vec3::new(0.8, 0.25, 0.8),
        EntityKind::Prowler => Vec3::new(0.5, 0.5, 0.5),
        EntityKind::TriggerOrb => Vec3::splat(0.45),
        EntityKind::RelayGate => Vec3::new(0.6, 1.1, 0.4),
        EntityKind::Checkpoint => Vec3::splat(0.45),
        EntityKind::Teleporter => Vec3::new(0.55, 0.3, 0.55),
        EntityKind::Fan => Vec3::splat(0.5),
        EntityKind::Bumper => Vec3::splat(0.55),
        EntityKind::Crate => Vec3::splat(0.5),
        EntityKind::Key => Vec3::splat(0.4),
        EntityKind::LockGate => Vec3::new(0.55, 1.2, 0.3),
        EntityKind::HealOrb => Vec3::splat(0.4),
        EntityKind::SpeedRing => Vec3::splat(0.6),
        EntityKind::CrumblePlate => Vec3::new(0.55, 0.15, 0.55),
        EntityKind::Cannon => Vec3::new(0.45, 0.45, 0.45),
        EntityKind::OnOffSwitch => Vec3::new(0.35, 0.15, 0.35),
        EntityKind::TossCrate => Vec3::splat(0.5),
        EntityKind::Sign => Vec3::new(0.5, 1.0, 0.3),
        EntityKind::Wedge => Vec3::splat(0.5),
    };
    let color = Color::srgb(0.3, 0.8, 1.0);
    let min = center - half;
    let max = center + half;
    let bottom = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, min.y, max.z),
    ];
    let top: Vec<Vec3> = bottom
        .iter()
        .map(|c| *c + Vec3::new(0.0, max.y - min.y, 0.0))
        .collect();
    gizmos.lineloop(bottom, color);
    gizmos.lineloop(top.iter().copied(), color);
    for i in 0..4 {
        gizmos.line(bottom[i], top[i], color);
    }
}

fn draw_aabb(gizmos: &mut Gizmos, center: Vec3, half: Vec3, color: Color) {
    let min = center - half;
    let max = center + half;

    let bottom = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, min.y, max.z),
    ];

    let top = [
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];

    gizmos.lineloop(bottom, color);
    gizmos.lineloop(top, color);

    for i in 0..4 {
        gizmos.line(bottom[i], top[i], color);
    }
}

pub fn draw_selection_gizmos(
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    selection: Res<SelectionSet>,
    mut gizmos: Gizmos,
) {
    if *mode != MakerMode::Edit || selection.is_empty() {
        return;
    }

    let block_color = Color::srgb(0.2, 0.9, 1.0);
    let entity_color = Color::srgb(1.0, 0.85, 0.25);

    for &cell in &selection.blocks {
        if level.get_block(cell).is_some() {
            let center = cell.as_vec3() + Vec3::splat(0.5);
            draw_aabb(&mut gizmos, center, Vec3::splat(0.54), block_color);
        }
    }

    for &id in &selection.entities {
        if let Some(entity) = level.entity_by_id(id) {
            let center = entity.cell_i().as_vec3() + Vec3::new(0.5, 0.5, 0.5);
            draw_aabb(&mut gizmos, center, Vec3::splat(0.62), entity_color);
        }
    }
}
