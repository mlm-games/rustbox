use bevy::prelude::*;

use crate::app::{OverlayMenu, Paused};
use game_utils_bevy::screen_effects::{FlashWhite, ScreenEffects, Trauma};

use super::block::BlockKind;
use super::campaign::{CampaignProgress, LevelSource};
use super::collision::overlaps_kind;
use super::entities_runtime::Prowler;
use super::level::{ClearCondition, LevelDocument};
use super::mode::MakerMode;
use super::player::Player;
use super::ui_bridge::MakerUi;

fn fmt_ms(ms: u32) -> String {
    format!("{}:{:02}.{:03}", ms / 60_000, (ms / 1_000) % 60, ms % 1_000)
}

fn clear_condition_blocker(
    cond: ClearCondition,
    ui: &MakerUi,
    clear_ms: u32,
    prowlers_remaining: usize,
) -> Option<String> {
    match cond {
        ClearCondition::ReachGoal => None,
        ClearCondition::CollectAllGlimmers => {
            if ui.glimmers_collected >= ui.glimmers_total {
                None
            } else {
                Some(format!(
                    "Collect all glimmers first ({}/{})",
                    ui.glimmers_collected, ui.glimmers_total
                ))
            }
        }
        ClearCondition::DefeatAllProwlers => {
            if prowlers_remaining == 0 {
                None
            } else {
                Some(format!(
                    "Defeat all prowlers first ({prowlers_remaining} left)"
                ))
            }
        }
        ClearCondition::NoDeath => {
            if ui.deaths == 0 {
                None
            } else {
                Some("Clear condition failed: no deaths allowed.".to_string())
            }
        }
        ClearCondition::TimeLimitMs(limit) => {
            if clear_ms <= limit {
                None
            } else {
                Some(format!("Too slow - finish under {}.", fmt_ms(limit)))
            }
        }
    }
}

pub fn detect_goal(
    mode: Res<MakerMode>,
    mut level: ResMut<LevelDocument>,
    mut ui: ResMut<MakerUi>,
    mut overlay: ResMut<OverlayMenu>,
    mut paused: ResMut<Paused>,
    mut trauma: ResMut<Trauma>,
    mut flash: ResMut<FlashWhite>,
    mut virtual_time: ResMut<Time<Virtual>>,
    source: Option<Res<LevelSource>>,
    mut progress: Option<ResMut<CampaignProgress>>,
    q: Query<(&Transform, &Player)>,
    prowlers: Query<(), With<Prowler>>,
) {
    if *mode != MakerMode::Play || ui.goal_latched {
        return;
    }

    for (tf, player) in &q {
        if !overlaps_kind(tf.translation, player.half_extents, &level, BlockKind::Goal) {
            continue;
        }

        let clear_ms = (ui.play_timer * 1000.0).round() as u32;
        let remaining_prowlers = prowlers.iter().count();

        if let Some(msg) = clear_condition_blocker(
            level.data.clear_condition,
            &ui,
            clear_ms,
            remaining_prowlers,
        ) {
            ui.set_status(msg);
            continue;
        }

        ui.goal_latched = true;
        ui.clear_time_secs = ui.play_timer;
        ui.clear_deaths = ui.deaths;

        let is_author = source.as_deref() == Some(&LevelSource::Editor);
        ui.player_is_author = is_author;

        if let (Some(s), Some(p)) = (&source, progress.as_deref_mut())
            && let LevelSource::Bundled(i) = **s
        {
            let id = super::campaign::BUNDLED_LEVELS[i].id;
            p.record_clear(id, ui.clear_time_secs, ui.clear_deaths);
        }

        if !level.data.is_verified {
            level.data.is_verified = true;
            if is_author {
                level.data.author_time = Some(clear_ms);
                level.data.author_deaths = ui.deaths;
                ui.first_clear = true;
                ui.new_record = false;
                ui.set_status("Level verified! First clear recorded.");
            } else {
                level.data.record_ms = Some(clear_ms);
                ui.first_clear = true;
                ui.new_record = true;
                ui.set_status("First clear! You set the world record.");
            }
        } else if !is_author {
            let better = level.data.record_ms.is_none_or(|r| clear_ms < r);
            if better {
                level.data.record_ms = Some(clear_ms);
                ui.new_record = true;
            } else {
                ui.new_record = false;
            }
            ui.first_clear = false;
        }

        ScreenEffects::add_trauma(&mut trauma, 0.45);
        ScreenEffects::flash_white(&mut flash, 0.25);

        paused.0 = true;
        virtual_time.pause();
        *overlay = OverlayMenu::LevelClear;
        ui.set_status("Level clear!");
        break;
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

pub fn on_mode_changed(
    mode: Res<MakerMode>,
    mut ui: ResMut<MakerUi>,
    mut level: ResMut<LevelDocument>,
    mut link: ResMut<super::entities_runtime::LinkState>,
) {
    if !mode.is_changed() {
        return;
    }

    if *mode == MakerMode::Play {
        ui.play_timer = 0.0;
        ui.goal_latched = false;
        ui.first_clear = false;
        ui.new_record = false;
        ui.player_is_author = false;
        ui.glimmers_collected = 0;
        ui.score = 0;
        level.entities_dirty = true;
        link.pulses.clear();
        link.clock = 0.0;
        ui.glimmers_total = level
            .data
            .entities
            .iter()
            .map(|e| {
                if e.kind == super::entity_data::EntityKind::Glimmer {
                    1
                } else if let super::entity_data::ContainedItem::Glimmers(n) = e.contents {
                    n as u32
                } else {
                    0
                }
            })
            .sum();
    }
}
