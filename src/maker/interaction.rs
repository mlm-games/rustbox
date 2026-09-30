use bevy_ecs::prelude::{Entity, Mut, World};
use glam::Vec3;

use super::Paused;
use super::block::BlockKind;
use super::collision::overlaps_kind;
use super::level::LevelDocument;
use super::mode::MakerMode;
use super::player::{MoveState, PlayIntent, Player, PlayerTransform, Trauma, respawn_player};
use super::win::MakerUi;

pub fn contact_he(player: &Player) -> Vec3 {
    if player.crouched {
        Vec3::new(
            player.half_extents.x,
            player.half_extents.y * 0.55,
            player.half_extents.z,
        )
    } else {
        player.half_extents
    }
}

fn respawn(world: &mut World, player: Entity) {
    world.resource_scope(|world, level: Mut<LevelDocument>| {
        let mut q = world.query::<(&mut PlayerTransform, &mut Player, &mut MoveState)>();
        if let Ok((mut transform, mut player_c, mut move_state)) = q.get_mut(world, player) {
            respawn_player(&mut transform, &mut player_c, &mut move_state, &level);
        }
    });
}

/// Hazards damage the player through the shared damage path; falling out of
/// bounds or manual reset request a full respawn.
pub fn play_hazard_goal(world: &mut World) {
    if *world.resource::<MakerMode>() != MakerMode::Play {
        return;
    }
    if world.resource::<Paused>().0 || world.resource::<MakerUi>().goal_latched {
        return;
    }
    enum Outcome {
        None,
        Respawn { manual_reset: bool },
        Damage,
    }
    let (player_e, outcome) = {
        let mut q = world.query::<(Entity, &PlayerTransform, &Player)>();
        let level = world.resource::<LevelDocument>();
        let intent = world.resource::<PlayIntent>();
        let Ok((player_e, transform, player)) = q.single(world) else {
            return;
        };
        let he = contact_he(player);
        let hit_hazard = overlaps_kind(transform.translation, he, &level, BlockKind::Hazard)
            || overlaps_kind(transform.translation, he, &level, BlockKind::Spikes);
        let fell_off = transform.translation.y < -20.0;
        let bounds = level.play_bounds();
        let out_of_bounds = transform.translation.x < bounds.0.x as f32 - 0.5
            || transform.translation.x > bounds.1.x as f32 + 0.5
            || transform.translation.z < bounds.0.z as f32 - 0.5
            || transform.translation.z > bounds.1.z as f32 + 0.5;
        let manual_reset = intent.reset_pressed;
        let outcome = if fell_off || out_of_bounds || manual_reset {
            Outcome::Respawn { manual_reset }
        } else if hit_hazard && player.invuln <= 0.0 {
            Outcome::Damage
        } else {
            Outcome::None
        };
        (player_e, outcome)
    };
    match outcome {
        Outcome::None => {}
        Outcome::Respawn { manual_reset } => {
            {
                let mut ui = world.resource_mut::<MakerUi>();
                if !manual_reset {
                    ui.deaths += 1;
                }
                ui.set_status(if manual_reset {
                    "Back to checkpoint!"
                } else {
                    "You fell off the level!"
                });
            }
            respawn(world, player_e);
        }
        Outcome::Damage => {
            {
                let mut ui = world.resource_mut::<MakerUi>();
                ui.deaths += 1;
                ui.set_status("Ouch!");
            }
            world.resource_mut::<Trauma>().add(0.35);
            respawn(world, player_e);
        }
    }
}
