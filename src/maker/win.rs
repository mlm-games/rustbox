use bevy_ecs::prelude::*;

use super::block::BlockKind;
use super::collision::overlaps_kind;
use super::entities_runtime::{LinkState, Prowler};
use super::entity_data::{ContainedItem, EntityKind};
use super::interaction::contact_he;
use super::level::{ClearCondition, LevelDocument};
use super::mode::MakerMode;
use super::player::{MoveState, PlayIntent, Player, PlayerTransform, Trauma};

#[derive(Resource, Debug, Default)]
pub struct MakerUi {
    pub play_timer: f32,
    pub goal_latched: bool,
    pub clear_time_secs: f32,
    pub deaths: u32,
    pub status: String,
    pub status_timer: f32,
    pub glimmers_collected: u32,
    pub glimmers_total: u32,
    pub sign_dialog_open: bool,
    pub sign_dialog_lines: Vec<String>,
    pub score: u32,
    /// Collection key of the level being edited; `None` saves to the
    /// autosave slot.
    pub current_key: Option<String>,
}

impl MakerUi {
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_timer = 2.5;
    }
}

pub fn fmt_ms(ms: u32) -> String {
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

pub fn detect_goal(world: &mut World, mode: MakerMode) {
    if mode != MakerMode::Play {
        return;
    }
    enum Decision {
        None,
        Blocked(String),
        Clear,
    }
    let decision = world.resource_scope(|world, level: Mut<LevelDocument>| {
        if world.resource::<MakerUi>().goal_latched {
            return Decision::None;
        }
        let remaining_prowlers = {
            let mut q = world.query_filtered::<Entity, With<Prowler>>();
            q.iter(world).count()
        };
        let mut q = world.query::<(&PlayerTransform, &Player)>();
        let ui = world.resource::<MakerUi>();
        let mut decision = Decision::None;
        for (tf, player) in q.iter(world) {
            if !overlaps_kind(tf.translation, contact_he(player), &level, BlockKind::Goal) {
                continue;
            }
            let clear_ms = (ui.play_timer * 1000.0).round() as u32;
            if let Some(msg) = clear_condition_blocker(
                level.data.clear_condition,
                ui,
                clear_ms,
                remaining_prowlers,
            ) {
                decision = Decision::Blocked(msg);
                continue;
            }
            decision = Decision::Clear;
            break;
        }
        decision
    });
    match decision {
        Decision::None => {}
        Decision::Blocked(msg) => {
            world.resource_mut::<MakerUi>().set_status(msg);
        }
        Decision::Clear => {
            let mut ui = world.resource_mut::<MakerUi>();
            ui.goal_latched = true;
            ui.clear_time_secs = ui.play_timer;
            drop(ui);
            let mut trauma = world.resource_mut::<Trauma>();
            trauma.add(0.45);
            world.resource_mut::<super::Paused>().0 = true;
            world.resource_mut::<MakerUi>().set_status("Level clear!");
        }
    }
}

pub fn tick_play_timer(world: &mut World, mode: MakerMode, dt: f32) {
    if mode != MakerMode::Play || dt <= 0.0 {
        return;
    }
    let paused = world.resource::<super::Paused>().0;
    let mut ui = world.resource_mut::<MakerUi>();
    if paused || ui.goal_latched {
        return;
    }
    ui.play_timer += dt;
}

pub fn tick_status(world: &mut World, dt: f32) {
    let mut ui = world.resource_mut::<MakerUi>();
    if ui.goal_latched || ui.status_timer <= 0.0 {
        return;
    }
    ui.status_timer -= dt;
    if ui.status_timer <= 0.0 {
        ui.status.clear();
    }
}

pub fn on_mode_changed(world: &mut World, prev_mode: MakerMode, mode: MakerMode) {
    if prev_mode == mode || mode != MakerMode::Play {
        return;
    }
    let glimmers_total: u32 = {
        let level = world.resource::<LevelDocument>();
        level
            .data
            .entities
            .iter()
            .map(|e| {
                if e.kind == EntityKind::Glimmer {
                    1
                } else if let ContainedItem::Glimmers(n) = e.contents {
                    n as u32
                } else {
                    0
                }
            })
            .sum()
    };
    {
        let mut level = world.resource_mut::<LevelDocument>();
        level.entities_dirty = true;
    }
    {
        let mut link = world.resource_mut::<LinkState>();
        link.pulses.clear();
        link.clock = 0.0;
    }
    let mut ui = world.resource_mut::<MakerUi>();
    ui.play_timer = 0.0;
    ui.deaths = 0;
    ui.goal_latched = false;
    ui.clear_time_secs = 0.0;
    ui.glimmers_collected = 0;
    ui.glimmers_total = glimmers_total;
    ui.score = 0;
    ui.sign_dialog_open = false;
    ui.sign_dialog_lines.clear();
}

pub fn retry_play(world: &mut World, mode: MakerMode) {
    if mode != MakerMode::Play || !world.resource::<super::Paused>().0 {
        return;
    }
    let reset = {
        let mut intent = world.resource_mut::<PlayIntent>();
        let pressed = intent.reset_pressed;
        intent.reset_pressed = false;
        pressed
    };
    if !reset {
        return;
    }
    world.resource_mut::<super::Paused>().0 = false;
    world.resource_scope(|world, mut level: Mut<LevelDocument>| {
        level.entities_dirty = true;
        {
            let mut ui = world.resource_mut::<MakerUi>();
            ui.goal_latched = false;
            ui.play_timer = 0.0;
            ui.deaths = 0;
            ui.glimmers_collected = 0;
            ui.score = 0;
            ui.clear_time_secs = 0.0;
            ui.status.clear();
            ui.status_timer = 0.0;
            ui.sign_dialog_open = false;
            ui.sign_dialog_lines.clear();
        }
        let ids: Vec<Entity> = world
            .query_filtered::<Entity, With<Player>>()
            .iter(world)
            .collect();
        let mut q = world.query::<(&mut PlayerTransform, &mut Player, &mut MoveState)>();
        for id in ids {
            if let Ok((mut tf, mut player, mut move_state)) = q.get_mut(world, id) {
                super::player::reset_player_run(&mut tf, &mut player, &mut move_state, &level);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use glam::{IVec3, Vec3};

    use super::*;
    use crate::maker::block::BlockShape;
    use crate::maker::interactive_blocks::{OnOffState, reset_onoff_state};
    use crate::maker::level::BlockData;
    use crate::maker::player::{MoveTuning, PressedLatch, spawn_player, sync_mode};
    use crate::maker::props::RuntimeSolids;
    use crate::maker::{Paused, not_paused};

    fn sim_with(paused: bool) -> repame_shell::Sim {
        let mut sim = repame_shell::Sim::with_default_step();
        let mut level = LevelDocument::default();
        level.set_block(
            IVec3::new(0, 2, 0),
            Some(BlockData {
                position: [0, 2, 0],
                kind: BlockKind::Goal,
                shape: BlockShape::Full,
                rot: 0,
                waterlogged: false,
            }),
        );
        spawn_player(&mut sim.world, &level);
        sim.world.insert_resource(level);
        sim.world.insert_resource(MakerMode::Play);
        sim.world.insert_resource(Paused(paused));
        sim.world.insert_resource(MakerUi::default());
        sim.world.insert_resource(Trauma::default());
        sim.world.insert_resource(PlayIntent::default());
        sim.world.insert_resource(PressedLatch::default());
        sim.world.insert_resource(MoveTuning::default());
        sim.world.insert_resource(RuntimeSolids::default());
        sim.world.insert_resource(OnOffState::default());
        sim.world
            .insert_resource(crate::maker::camera::CameraRig::default());
        sim.add_chained_systems((sync_mode, reset_onoff_state).chain().run_if(not_paused));
        sim
    }

    #[test]
    fn fixed_chain_gated_by_not_paused() {
        let mut sim = sim_with(true);
        sim.step(Duration::from_millis(17));
        let player = {
            let mut q = sim.world.query_filtered::<Entity, With<Player>>();
            q.single(&sim.world).unwrap()
        };
        assert!(
            !sim.world.get::<PlayerTransform>(player).unwrap().visible,
            "paused must freeze the fixed chain"
        );
        sim.world.resource_mut::<Paused>().0 = false;
        sim.step(Duration::from_millis(17));
        assert!(
            sim.world.get::<PlayerTransform>(player).unwrap().visible,
            "the chain must resume once unpaused"
        );
    }

    #[test]
    fn detect_goal_latches_once_and_pauses() {
        let mut sim = sim_with(false);
        let world = &mut sim.world;
        let player = {
            let mut q = world.query_filtered::<Entity, With<Player>>();
            q.single(world).unwrap()
        };
        let spawn = world.get::<PlayerTransform>(player).unwrap().translation;

        detect_goal(world, MakerMode::Edit);
        {
            let ui = world.resource::<MakerUi>();
            assert!(!ui.goal_latched);
            assert_eq!(ui.clear_time_secs, 0.0);
        }

        world
            .get_mut::<PlayerTransform>(player)
            .unwrap()
            .translation = Vec3::new(500.0, 100.0, 500.0);
        detect_goal(world, MakerMode::Play);
        {
            let ui = world.resource::<MakerUi>();
            assert!(!ui.goal_latched, "off the goal must not latch");
        }

        world
            .get_mut::<PlayerTransform>(player)
            .unwrap()
            .translation = spawn;
        world.resource_mut::<MakerUi>().play_timer = 3.25;
        detect_goal(world, MakerMode::Play);
        {
            let ui = world.resource::<MakerUi>();
            assert!(ui.goal_latched);
            assert_eq!(ui.clear_time_secs, 3.25);
            assert_eq!(ui.status, "Level clear!");
            assert_eq!(ui.status_timer, 2.5);
        }
        assert!(world.resource::<Paused>().0);

        tick_play_timer(world, MakerMode::Play, 1.0);
        assert_eq!(world.resource::<MakerUi>().play_timer, 3.25);

        world.resource_mut::<MakerUi>().play_timer = 7.5;
        detect_goal(world, MakerMode::Play);
        assert_eq!(world.resource::<MakerUi>().clear_time_secs, 3.25);
    }
}
