use bevy_ecs::prelude::{Res, Resource};

pub mod block;
pub mod camera;
pub mod chunk;
pub mod collision;
pub mod commands;
pub mod entities_runtime;
pub mod entity_data;
pub mod interaction;
pub mod interactive_blocks;
pub mod level;
pub mod level_file;
pub mod level_view;
pub mod mode;
pub mod player;
pub mod props;
pub mod rapier;
pub mod track;
pub mod win;

#[derive(Resource, Default)]
pub struct Paused(pub bool);

pub fn not_paused(p: Res<Paused>) -> bool {
    !p.0
}
