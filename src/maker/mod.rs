pub mod block;
pub mod camera;
pub mod campaign;
pub mod catalog;
pub mod chunk;
pub mod collision;
pub mod commands;
pub mod cursor;
pub mod editor;
pub mod entities_runtime;
pub mod entity_data;
pub mod level;
pub mod mode;
pub mod online;
pub mod player;
#[cfg(feature = "physics")]
pub mod rapier;
pub mod rendering;
pub mod storage;
pub mod theme;
pub mod thumbnail;
pub mod track;
pub mod ui_bridge;
pub mod win;

use bevy::prelude::*;

use crate::app::{AppState, Paused};
use game_utils_bevy::transitions::Transition;

pub use mode::MakerStats;

use camera::CameraRig;
use campaign::LevelSource;
use commands::CommandHistory;
use entities_runtime::{EntityEntities, RuntimeSolids};
use level::LevelDocument;
use mode::{BlockBrush, BrushTab, InputCapture, MakerMode, PlaceYaw, SelectedEntityKind};
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
fn not_in_paste_preview(preview: Res<mode::PastePreview>) -> bool {
    !preview.active
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
        app.add_plugins(online::OnlinePlugin);
        #[cfg(feature = "physics")]
        app.add_plugins(crate::maker::rapier::rapier_plugin);
        app.init_resource::<MakerMode>()
            .init_resource::<BlockBrush>()
            .init_resource::<SelectedEntityKind>()
            .init_resource::<BrushTab>()
            .init_resource::<PlaceYaw>()
            .init_resource::<InputCapture>()
            .init_resource::<CommandHistory>()
            .init_resource::<LevelDocument>()
            .init_resource::<MakerStats>()
            .init_resource::<ActiveTrack>()
            .init_resource::<mode::SelectedEntity>()
            .init_resource::<mode::MirrorMode>()
            .init_resource::<mode::BoxFillStart>()
            .init_resource::<mode::EditorCursor>()
            .init_resource::<mode::ActiveLinkChannel>()
            .init_resource::<mode::SelectionSet>()
            .init_resource::<mode::SelectionBoxStart>()
            .init_resource::<mode::EditorClipboard>()
            .init_resource::<mode::PastePreview>()
            .init_resource::<entities_runtime::LinkState>()
            .init_resource::<campaign::LevelSource>()
            .init_resource::<campaign::CampaignProgress>()
            .init_resource::<rendering::WaterBoundaryState>()
            .add_message::<mode::BlockPlaced>()
            .init_resource::<CameraRig>()
            .init_resource::<ChunkEntities>()
            .init_resource::<rendering::WaterChunkEntities>()
            .init_resource::<EntityEntities>()
            .init_resource::<RuntimeSolids>()
            .init_resource::<player::MoveTuning>()
            .init_resource::<storage::LevelStorage>()
            .init_resource::<ui_bridge::MakerUi>()
            .init_resource::<entities_runtime::ClipLibrary>()
            .add_systems(Startup, campaign::load_campaign_progress)
            .add_systems(Update, campaign::save_campaign_progress)
            .add_systems(Startup, entities_runtime::init_clip_library)
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
            .add_systems(
                OnExit(AppState::InGame),
                (cleanup_maker, cursor::restore_cursor),
            )
            .add_systems(
                Update,
                (
                    editor::toggle_mode,
                    editor::block_palette_hotkeys
                        .run_if(in_edit)
                        .run_if(not_in_paste_preview),
                    editor::entity_palette_hotkeys
                        .run_if(in_edit)
                        .run_if(not_in_paste_preview),
                    editor::track_tool_hotkeys
                        .run_if(in_edit)
                        .run_if(not_in_paste_preview),
                    editor::undo_redo_hotkeys.run_if(in_edit),
                    editor::mirror_hotkey
                        .run_if(in_edit)
                        .run_if(not_in_paste_preview),
                    editor::update_preview_and_edit
                        .run_if(in_edit)
                        .run_if(not_in_paste_preview),
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
                    .run_if(not_blocked)
                    .after(ui_bridge::update_input_capture),
            )
            .add_systems(
                Update,
                (
                    editor::selection_hotkeys
                        .run_if(in_edit)
                        .after(ui_bridge::update_input_capture)
                        .before(editor::update_preview_and_edit),
                    editor::update_paste_preview
                        .run_if(in_edit)
                        .after(editor::update_preview_and_edit),
                ),
            )
            .add_systems(
                Update,
                (
                    track::draw_track_gizmos.run_if(in_edit),
                    editor::delete_selected_entity.run_if(in_edit),
                    editor::draw_selected_entity_gizmo.run_if(in_edit),
                    editor::draw_selection_gizmos.run_if(in_edit),
                    editor::draw_box_fill_preview.run_if(in_edit),
                    editor::draw_paste_preview_gizmos.run_if(in_edit),
                    editor::update_placement_preview.run_if(in_edit),
                    entities_runtime::draw_link_gizmos.run_if(in_edit),
                    entities_runtime::move_prowlers.after(entities_runtime::tick_track_followers),
                    entities_runtime::prowler_touch
                        .after(player::player_controller)
                        .after(entities_runtime::move_prowlers)
                        .run_if(in_play),
                    entities_runtime::trigger_orbs.run_if(in_play),
                    entities_runtime::update_relay_gates
                        .after(entities_runtime::trigger_orbs)
                        .run_if(in_play),
                    camera::frame_selection_hotkey.run_if(in_edit),
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
                    editor::update_editor_cursor.before(editor::update_preview_and_edit),
                    editor::spawn_place_ghosts.run_if(in_edit),
                    ui_bridge::push_ui_state,
                    ui_bridge::share_text_input.before(ui_bridge::drain_ui_commands),
                    ui_bridge::level_info_text_input.before(ui_bridge::drain_ui_commands),
                    cursor::cursor_policy,
                    storage::save_load_hotkeys
                        .run_if(in_edit)
                        .after(ui_bridge::update_input_capture),
                    win::tick_play_timer,
                    win::on_mode_changed,
                    win::detect_goal.run_if(in_play),
                    entities_runtime::apply_model_materials,
                    entities_runtime::apply_model_anims,
                    entities_runtime::tick_model_anims,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(
                Update,
                (
                    rendering::rebuild_water_and_boundary.before(rendering::rebuild_dirty_chunks),
                    rendering::apply_theme,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(Update, entities_runtime::collect_clips)
            .add_systems(
                Update,
                (
                    ui_bridge::clear_text_capture_when_not_browsing,
                    ui_bridge::browser_grid_nav.after(ui_bridge::update_input_capture),
                ),
            )
            .add_systems(
                OnEnter(AppState::InGame),
                |mut ui: ResMut<ui_bridge::MakerUi>, storage: Res<storage::LevelStorage>| {
                    ui.level_slots = storage::list_slots(&storage);
                },
            );
    }
}

fn setup_maker(
    mut level: ResMut<LevelDocument>,
    source: Res<LevelSource>,
    mut mode: ResMut<MakerMode>,
) {
    if *source == LevelSource::Editor && level.data.blocks.is_empty() {
        level.seed_default();
    }
    level.mark_all_dirty();
    level.entities_dirty = true;
    *mode = MakerMode::Edit;
}

fn cleanup_maker(
    mut commands: Commands,
    q: Query<Entity, With<MakerCleanup>>,
    mut chunks: ResMut<ChunkEntities>,
    mut water_chunks: ResMut<rendering::WaterChunkEntities>,
    mut entities: ResMut<EntityEntities>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
    chunks.0.clear();
    water_chunks.0.clear();
    entities.0.clear();
    commands.remove_resource::<rendering::MakerAssets>();
    commands.remove_resource::<entities_runtime::EntityAssets>();
}
