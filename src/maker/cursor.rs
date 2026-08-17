use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::app::OverlayMenu;

use super::mode::MakerMode;

pub fn restore_cursor(
    windows: Query<Entity, With<PrimaryWindow>>,
    mut cursor_opts: Query<&mut CursorOptions>,
) {
    let Ok(window_entity) = windows.single() else {
        return;
    };
    let Ok(mut cursor) = cursor_opts.get_mut(window_entity) else {
        return;
    };
    cursor.grab_mode = CursorGrabMode::None;
    cursor.visible = true;
}

pub fn cursor_policy(
    mode: Res<MakerMode>,
    paused: Res<crate::app::Paused>,
    buttons: Res<ButtonInput<MouseButton>>,
    overlay: Res<OverlayMenu>,
    windows: Query<Entity, With<PrimaryWindow>>,
    mut cursor_opts: Query<&mut CursorOptions>,
) {
    let Ok(window_entity) = windows.single() else {
        return;
    };
    let Ok(mut cursor) = cursor_opts.get_mut(window_entity) else {
        return;
    };

    // Any open overlay (pause, settings, level clear, sign editor, ...)
    // releases the lock so menus can use the mouse; the lock must come back
    // the moment the overlay closes, without waiting for a click.
    let ui_open = *overlay != OverlayMenu::None;
    let want_lock = *mode == MakerMode::Play && !paused.0 && !ui_open;

    let should_apply = mode.is_changed()
        || paused.is_changed()
        || overlay.is_changed()
        || (want_lock && buttons.just_pressed(MouseButton::Left));

    if !should_apply {
        return;
    }

    if want_lock {
        cursor.grab_mode = CursorGrabMode::Locked;
        cursor.visible = false;
    } else {
        cursor.grab_mode = CursorGrabMode::None;
        cursor.visible = true;
    }
}
