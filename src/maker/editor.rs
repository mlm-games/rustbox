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
            BrushTab::Entities => BrushTab::Blocks,
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

    if let Ok((mut tr, mut vis)) = preview_q.single_mut() {
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
                    if data.kind == EntityKind::DriftPlate {
                        let forward = Quat::from_rotation_y(place_yaw.0.to_radians()) * Vec3::NEG_Z;
                        let end = place_cell
                            + IVec3::new(
                                (forward.x.round() as i32) * 4,
                                0,
                                (forward.z.round() as i32) * 4,
                            );
                        data.cell_b = Some(end.to_array());
                    }
                    history.apply(&mut level, EditCommand::PlaceEntity { entity: data });
                }
            }
        }
    } else if buttons.just_pressed(MouseButton::Right) {
        if let Some(prev) = level.get_block(hit_cell) {
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
