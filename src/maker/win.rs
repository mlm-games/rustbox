use bevy::prelude::*;

use crate::app::{OverlayMenu, Paused};
use game_utils_bevy::screen_effects::{FlashWhite, ScreenEffects, Trauma};

use super::block::BlockKind;
use super::collision::overlaps_kind;
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::Player;
use super::ui_bridge::MakerUi;

pub fn detect_goal(
    mode: Res<MakerMode>,
    level: Res<LevelDocument>,
    mut ui: ResMut<MakerUi>,
    mut overlay: ResMut<OverlayMenu>,
    mut paused: ResMut<Paused>,
    mut trauma: ResMut<Trauma>,
    mut flash: ResMut<FlashWhite>,
    mut virtual_time: ResMut<Time<Virtual>>,
    q: Query<(&Transform, &Player)>,
) {
    if *mode != MakerMode::Play || ui.goal_latched {
        return;
    }
    for (tf, player) in &q {
        if overlaps_kind(tf.translation, player.half_extents, &level, BlockKind::Goal) {
            ui.goal_latched = true;
            ui.clear_time_secs = ui.play_timer;
            ui.clear_deaths = ui.deaths;

            ScreenEffects::add_trauma(&mut trauma, 0.45);
            ScreenEffects::flash_white(&mut flash, 0.25);

            paused.0 = true;
            virtual_time.pause();
            *overlay = OverlayMenu::LevelClear;
            ui.set_status("Level clear!");
            break;
        }
    }
}

pub fn tick_play_timer(
    time: Res<Time>,
    mode: Res<MakerMode>,
    paused: Res<Paused>,
    mut ui: ResMut<MakerUi>,
) {
    if *mode == MakerMode::Play && !paused.0 && !ui.goal_latched {
        ui.play_timer += time.delta_secs();
    }
}

pub fn on_mode_changed(mode: Res<MakerMode>, mut ui: ResMut<MakerUi>, level: Res<LevelDocument>) {
    if !mode.is_changed() {
        return;
    }
    if *mode == MakerMode::Play {
        ui.play_timer = 0.0;
        ui.goal_latched = false;
        ui.glimmers_collected = 0;
        ui.glimmers_total = level
            .data
            .entities
            .iter()
            .filter(|e| e.kind == super::entity_data::EntityKind::Glimmer)
            .count() as u32;
    }
}
