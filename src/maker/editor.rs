use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::block::BlockKind;
use super::camera::WorldCamera;
use super::collision::raycast_present;
use super::commands::{CommandHistory, EditCommand};
use super::entity_data::{EntityData, EntityKind};
use super::level::LevelDocument;
use super::mode::{
    BrushTab, InputCapture, MakerMode, MakerStats, PlaceYaw, SelectedBlockKind, SelectedEntityKind,
};
use super::rendering::{MakerAssets, PlacementPreview, spawn_place_ghost};
use super::track::{ActiveTrack, TrackData, TrackMode};

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

pub fn update_preview_and_edit(
    mut commands: Commands,
    capture: Res<InputCapture>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam_q: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    selected: Res<SelectedBlockKind>,
    sel_e: Res<SelectedEntityKind>,
    tab: Res<BrushTab>,
    place_yaw: Res<PlaceYaw>,
    mut active: ResMut<ActiveTrack>,
    mut level: ResMut<LevelDocument>,
    mut history: ResMut<CommandHistory>,
    mut stats: ResMut<MakerStats>,
    mut preview_q: Query<(&mut Transform, &mut Visibility), With<PlacementPreview>>,
    assets: Option<Res<MakerAssets>>,
) {
    let hide = |q: &mut Query<(&mut Transform, &mut Visibility), With<PlacementPreview>>| {
        if let Ok((_, mut vis)) = q.single_mut() {
            *vis = Visibility::Hidden;
        }
    };

    if capture.ui_wants_pointer {
        hide(&mut preview_q);
        return;
    }
    let Ok(window) = windows.single() else {
        hide(&mut preview_q);
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        hide(&mut preview_q);
        return;
    };
    let Ok((camera, cam_tf)) = cam_q.single() else {
        hide(&mut preview_q);
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else {
        hide(&mut preview_q);
        return;
    };

    let Some((hit_cell, normal)) = raycast_present(&level, ray.origin, *ray.direction, 200.0)
    else {
        hide(&mut preview_q);
        return;
    };

    let place_cell = hit_cell + normal;
    let center = Vec3::new(
        place_cell.x as f32 + 0.5,
        place_cell.y as f32 + 0.5,
        place_cell.z as f32 + 0.5,
    );

    if *tab != BrushTab::Tracks
        && let Ok((mut tr, mut vis)) = preview_q.single_mut()
    {
        tr.translation = center;
        *vis = Visibility::Visible;
    }

    if buttons.just_pressed(MouseButton::Left) {
        match *tab {
            BrushTab::Blocks => {
                if level.get_block(place_cell).is_none() {
                    history.apply(
                        &mut level,
                        EditCommand::Place {
                            position: place_cell,
                            kind: selected.0,
                            previous: None,
                        },
                    );
                    stats.blocks_placed += 1;
                    if let Some(ref assets) = assets {
                        spawn_place_ghost(&mut commands, assets, place_cell, selected.0);
                    }
                }
            }
            BrushTab::Entities => {
                if level.entity_at_cell(place_cell).is_none() {
                    let id = level.alloc_id();
                    let mut data = EntityData::defaults_for(sel_e.0, place_cell, id);
                    data.yaw_deg = place_yaw.0;
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
        } else if let Some(prev) = level.get_block(hit_cell) {
            history.apply(
                &mut level,
                EditCommand::Remove {
                    position: hit_cell,
                    previous: prev,
                },
            );
        } else if let Some(ent) = level.entity_at_cell(hit_cell).cloned() {
            history.apply(&mut level, EditCommand::RemoveEntity { entity: ent });
        }
    }
}
