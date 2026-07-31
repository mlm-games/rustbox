pub mod block;
pub mod camera;
pub mod chunk;
pub mod collision;
pub mod commands;
pub mod cursor;
pub mod editor;
pub mod entities_runtime;
pub mod entity_data;
pub mod level;
pub mod mode;
pub mod player;
#[cfg(feature = "physics")]
pub mod rapier;
pub mod rendering;
pub mod storage;
pub mod track;
pub mod ui_bridge;
pub mod win;

use bevy::prelude::*;

use crate::app::{AppState, Paused};
use game_utils_bevy::transitions::Transition;

pub use mode::MakerStats;

use camera::CameraRig;
use commands::CommandHistory;
use entities_runtime::{EntityEntities, RuntimeSolids};
use level::LevelDocument;
use mode::{BrushTab, InputCapture, MakerMode, PlaceYaw, SelectedBlockKind, SelectedEntityKind};
use rendering::ChunkEntities;
use track::ActiveTrack;

#[derive(Component)]
pub struct MakerCleanup;

fn in_edit(mode: Res<MakerMode>) -> bool {
    *mode == MakerMode::Edit
}
fn in_play(mode: Res<MakerMode>) -> bool {
    *mode == MakerMode::Play
}
fn not_paused(p: Res<Paused>) -> bool {
    !p.0
}
fn not_blocked(t: Res<Transition<AppState>>) -> bool {
    !t.block_input
}

pub struct MakerPlugin;

impl Plugin for MakerPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "physics")]
        app.add_plugins(crate::maker::rapier::rapier_plugin);
        app.init_resource::<MakerMode>()
            .init_resource::<SelectedBlockKind>()
            .init_resource::<SelectedEntityKind>()
            .init_resource::<BrushTab>()
            .init_resource::<PlaceYaw>()
            .init_resource::<InputCapture>()
            .init_resource::<CommandHistory>()
            .init_resource::<LevelDocument>()
            .init_resource::<MakerStats>()
            .init_resource::<ActiveTrack>()
            .init_resource::<CameraRig>()
            .init_resource::<ChunkEntities>()
            .init_resource::<EntityEntities>()
            .init_resource::<RuntimeSolids>()
            .init_resource::<storage::LevelStorage>()
            .init_resource::<ui_bridge::MakerUi>()
            .add_systems(
                OnEnter(AppState::InGame),
                (
                    setup_maker,
                    rendering::setup_world,
                    entities_runtime::setup_entity_assets,
                    camera::spawn_camera,
                )
                    .chain(),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_maker)
            .add_systems(
                Update,
                (
                    editor::toggle_mode,
                    editor::block_palette_hotkeys.run_if(in_edit),
                    editor::entity_palette_hotkeys.run_if(in_edit),
                    editor::track_tool_hotkeys.run_if(in_edit),
                    editor::undo_redo_hotkeys.run_if(in_edit),
                    editor::update_preview_and_edit.run_if(in_edit),
                    rendering::rebuild_dirty_chunks,
                    rendering::tick_ghosts,
                    entities_runtime::reconcile_entities,
                    entities_runtime::bob_glimmers,
                    entities_runtime::tick_launch_pads_cooldown,
                    entities_runtime::tick_track_followers.before(player::player_controller),
                    entities_runtime::tick_drift_plates.before(player::player_controller),
                    entities_runtime::rebuild_runtime_solids,
                    player::sync_mode,
                    player::player_controller.run_if(in_play),
                    player::play_hazard_goal.run_if(in_play),
                    entities_runtime::collect_glimmers.run_if(in_play),
                    entities_runtime::update_seals.run_if(in_play),
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked),
            )
            .add_systems(
                Update,
                (
                    track::draw_track_gizmos.run_if(in_edit),
                    entities_runtime::move_prowlers.after(entities_runtime::tick_track_followers),
                    entities_runtime::prowler_touch
                        .after(player::player_controller)
                        .after(entities_runtime::move_prowlers)
                        .run_if(in_play),
                    camera::edit_camera_control.run_if(in_edit),
                    camera::play_camera_follow.run_if(in_play),
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked),
            )
            .add_systems(
                Update,
                (
                    (
                        ui_bridge::update_input_capture,
                        ui_bridge::drain_ui_commands,
                    )
                        .chain()
                        .before(editor::update_preview_and_edit),
                    ui_bridge::push_ui_state,
                    cursor::cursor_policy,
                    storage::save_load_hotkeys.run_if(in_edit),
                    win::tick_play_timer,
                    win::on_mode_changed,
                    win::detect_goal.run_if(in_play),
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                |mut ui: ResMut<ui_bridge::MakerUi>, storage: Res<storage::LevelStorage>| {
                    ui.level_slots = storage.0.list().unwrap_or_default();
                },
            );
    }
}

fn setup_maker(mut level: ResMut<LevelDocument>, mut mode: ResMut<MakerMode>) {
    level.seed_default();
    *mode = MakerMode::Edit;
}

fn cleanup_maker(
    mut commands: Commands,
    q: Query<Entity, With<MakerCleanup>>,
    mut chunks: ResMut<ChunkEntities>,
    mut entities: ResMut<EntityEntities>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
    chunks.0.clear();
    entities.0.clear();
    commands.remove_resource::<rendering::MakerAssets>();
    commands.remove_resource::<entities_runtime::EntityAssets>();
}
