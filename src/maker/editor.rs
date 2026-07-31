use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::block::BlockKind;
use super::camera::WorldCamera;
use super::collision::raycast_present;
use super::commands::{CommandHistory, EditCommand};
use super::entity_data::{EntityData, EntityKind};
use super::level::LevelDocument;
use super::mode::{
    ActiveLinkChannel, BlockPlaced, BoxFillStart, BrushTab, EditorCursor, InputCapture, MakerMode,
    MakerStats, MirrorMode, PlaceYaw, SelectedBlockKind, SelectedEntity, SelectedEntityKind,
};
use super::rendering::{MakerAssets, PlacementPreview, spawn_place_ghost};
use super::track::{ActiveTrack, TrackData, TrackMode};
use super::ui_bridge::MakerUi;

pub fn toggle_mode(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<MakerMode>) {
    if keys.just_pressed(KeyCode::Tab) {
        *mode = match *mode {
            MakerMode::Edit => MakerMode::Play,
            MakerMode::Play => MakerMode::Edit,
        };
    }
}

pub fn block_palette_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    tab: Res<BrushTab>,
    mut selected: ResMut<SelectedBlockKind>,
) {
    if *tab != BrushTab::Blocks {
        return;
    }
    if keys.just_pressed(KeyCode::Digit1) {
        selected.0 = BlockKind::Grass;
    }
    if keys.just_pressed(KeyCode::Digit2) {
        selected.0 = BlockKind::Stone;
    }
    if keys.just_pressed(KeyCode::Digit3) {
        selected.0 = BlockKind::Hazard;
    }
    if keys.just_pressed(KeyCode::Digit4) {
        selected.0 = BlockKind::Goal;
    }
    if keys.just_pressed(KeyCode::Digit5) {
        selected.0 = BlockKind::Spawn;
    }
}

pub fn entity_palette_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut tab: ResMut<BrushTab>,
    mut sel_e: ResMut<SelectedEntityKind>,
    mut place_yaw: ResMut<PlaceYaw>,
    mut channel: ResMut<ActiveLinkChannel>,
    mut ui: ResMut<MakerUi>,
) {
    if keys.just_pressed(KeyCode::KeyQ) {
        *tab = match *tab {
            BrushTab::Blocks => BrushTab::Entities,
            BrushTab::Entities => BrushTab::Tracks,
            BrushTab::Tracks => BrushTab::Blocks,
        };
    }
    if keys.just_pressed(KeyCode::KeyF) {
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
}

pub fn track_tool_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    tab: Res<BrushTab>,
    mut active: ResMut<ActiveTrack>,
    mut history: ResMut<CommandHistory>,
    mut level: ResMut<LevelDocument>,
) {
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
    mut mirror: ResMut<MirrorMode>,
    mut ui: ResMut<MakerUi>,
) {
    if keys.just_pressed(KeyCode::KeyV) {
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

pub fn undo_redo_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut history: ResMut<CommandHistory>,
    mut level: ResMut<LevelDocument>,
) {
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
    cursor.hit = None;
    cursor.place = None;
    if let Some((hit, normal)) = raycast_present(&level, ray.origin, *ray.direction, 200.0) {
        cursor.hit = Some(hit);
        cursor.place = Some(hit + normal);
    }
}

/// Moves the placement ghost to the cursor's target cell (Blocks/Entities tabs).
pub fn update_placement_preview(
    cursor: Res<EditorCursor>,
    tab: Res<BrushTab>,
    mut preview_q: Query<(&mut Transform, &mut Visibility), With<PlacementPreview>>,
) {
    let Ok((mut tr, mut vis)) = preview_q.single_mut() else {
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
    tr.translation = Vec3::new(
        place_cell.x as f32 + 0.5,
        place_cell.y as f32 + 0.5,
        place_cell.z as f32 + 0.5,
    );
    *vis = Visibility::Visible;
}

pub fn update_preview_and_edit(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    cursor: Res<EditorCursor>,
    mut selected: ResMut<SelectedBlockKind>,
    mut sel_e: ResMut<SelectedEntityKind>,
    mut tab: ResMut<BrushTab>,
    place_yaw: Res<PlaceYaw>,
    mirror: Res<MirrorMode>,
    mut box_start: ResMut<BoxFillStart>,
    mut active: ResMut<ActiveTrack>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
    mut stats: ResMut<MakerStats>,
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

    // Eyedropper: middle-click picks the block/entity under the cursor.
    if buttons.just_pressed(MouseButton::Middle) {
        if let Some(kind) = level.get_block(hit_cell) {
            selected.0 = kind;
            *tab = BrushTab::Blocks;
        } else if let Some(ent) = level.entity_at_cell(hit_cell) {
            sel_e.0 = ent.kind;
            *tab = BrushTab::Entities;
        }
    }

    if buttons.just_pressed(MouseButton::Left) {
        match *tab {
            BrushTab::Blocks => {
                if shift {
                    match box_start.0 {
                        None => box_start.0 = Some(place_cell),
                        Some(a) => {
                            box_start.0 = None;
                            let min = a.min(place_cell);
                            let max = a.max(place_cell);
                            let mut cells = Vec::new();
                            for x in min.x..=max.x {
                                for y in min.y..=max.y {
                                    for z in min.z..=max.z {
                                        let c = IVec3::new(x, y, z);
                                        let prev = level.get_block(c);
                                        if prev != Some(selected.0) {
                                            cells.push((c, prev));
                                        }
                                    }
                                }
                            }
                            if !cells.is_empty() {
                                history.apply(
                                    &mut level,
                                    EditCommand::BoxFill {
                                        cells,
                                        kind: selected.0,
                                    },
                                );
                            }
                        }
                    }
                } else {
                    for cell in mirror_cells(place_cell, mirror.0) {
                        if level.get_block(cell).is_none() {
                            history.apply(
                                &mut level,
                                EditCommand::Place {
                                    position: cell,
                                    kind: selected.0,
                                    previous: None,
                                },
                            );
                            stats.blocks_placed += 1;
                            placed.write(BlockPlaced {
                                cell,
                                kind: selected.0,
                            });
                        }
                    }
                }
            }
            BrushTab::Entities => {
                if let Some(ent) = level.entity_at_cell(place_cell) {
                    sel_ent.0 = Some(ent.id);
                } else {
                    let id = level.alloc_id();
                    let mut data = EntityData::defaults_for(sel_e.0, place_cell, id);
                    data.yaw_deg = place_yaw.0;
                    if matches!(data.kind, EntityKind::TriggerOrb | EntityKind::RelayGate) {
                        data.link = channel.0;
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
                } else {
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
                if let Some(k) = level.get_block(cell) {
                    history.apply(
                        &mut level,
                        EditCommand::Remove {
                            position: cell,
                            previous: k,
                        },
                    );
                    stats.blocks_placed = stats.blocks_placed.saturating_sub(1);
                }
            }
        } else if let Some(ent) = level.entity_at_cell(hit_cell).cloned() {
            let removed_id = ent.id;
            history.apply(&mut level, EditCommand::RemoveEntity { entity: ent });
            if sel_ent.0 == Some(removed_id) {
                sel_ent.0 = None;
            }
        }
    }

    // Drag paint: hold LMB to keep placing, RMB to keep erasing.
    if *tab == BrushTab::Blocks && !shift {
        if buttons.pressed(MouseButton::Left) {
            for cell in mirror_cells(place_cell, mirror.0) {
                if level.get_block(cell).is_none() {
                    history.apply(
                        &mut level,
                        EditCommand::Place {
                            position: cell,
                            kind: selected.0,
                            previous: None,
                        },
                    );
                    stats.blocks_placed += 1;
                    placed.write(BlockPlaced {
                        cell,
                        kind: selected.0,
                    });
                }
            }
        } else if buttons.pressed(MouseButton::Right) {
            for cell in mirror_cells(hit_cell, mirror.0) {
                if let Some(k) = level.get_block(cell) {
                    history.apply(
                        &mut level,
                        EditCommand::Remove {
                            position: cell,
                            previous: k,
                        },
                    );
                    stats.blocks_placed = stats.blocks_placed.saturating_sub(1);
                }
            }
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
        spawn_place_ghost(&mut commands, &assets, ev.cell, ev.kind);
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
