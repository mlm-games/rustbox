pub mod asset_manifest;
pub mod block;
pub mod block_asset_manifest;
pub mod camera;
pub mod campaign;
pub mod catalog;
pub mod chunk;
pub mod collision;
pub mod commands;
pub mod creator;
pub mod cursor;
pub mod editor;
pub mod entities_runtime;
pub mod entity_data;
pub mod interaction;
pub mod interactive_blocks;
pub mod level;
pub mod limits;
pub mod mode;
pub mod online;
pub mod player;
pub mod rapier;
pub mod rendering;
pub mod storage;
pub mod theme;
pub mod thumbnail;
pub mod track;
pub mod ui_bridge;
pub mod win;

use bevy::prelude::*;
use bevy_rapier3d::prelude::PhysicsSet;
use std::path::PathBuf;

use crate::app::{AppState, Paused};
use game_utils_bevy::transitions::Transition;

/// Bevy's asset root: `<cargo manifest dir>/assets`. The file reader defaults
/// to this directory, so RON preview/model paths (relative to `assets/`) can
/// be validated against the disk here.
fn asset_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

pub use mode::MakerStats;

use camera::CameraRig;
use campaign::LevelSource;
use commands::CommandHistory;
use entities_runtime::{EntityEntities, RuntimeSolids};
use interaction::InteractionSet;
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
        app.add_plugins(crate::maker::rapier::rapier_plugin);
        app.insert_resource(block_asset_manifest::BlockAssetManifest::load(&asset_root()));
        app.insert_resource(asset_manifest::EntityModelManifest::load(&asset_root()));
        app.init_resource::<MakerMode>()
            .init_resource::<BlockBrush>()
            .init_resource::<SelectedEntityKind>()
            .init_resource::<BrushTab>()
            .init_resource::<PlaceYaw>()
            .init_resource::<InputCapture>()
            .init_resource::<CommandHistory>()
            .init_resource::<LevelDocument>()
            .init_resource::<MakerStats>()
            .init_resource::<limits::LevelLimits>()
            .init_resource::<limits::LevelStats>()
            .init_resource::<interactive_blocks::OnOffState>()
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
            .init_resource::<interaction::InteractionMemory>()
            .init_resource::<interaction::ForcedMotionRequests>()
            .init_resource::<interaction::DamageRequests>()
            .init_resource::<interaction::UseSelection>()
            .init_resource::<campaign::LevelSource>()
            .init_resource::<campaign::CampaignProgress>()
            .init_resource::<rendering::WaterBoundaryState>()
            .add_message::<mode::BlockPlaced>()
            .init_resource::<CameraRig>()
            .init_resource::<ChunkEntities>()
            .init_resource::<rendering::WaterChunkEntities>()
            .init_resource::<rendering::BlockOverlayEntities>()
            .init_resource::<EntityEntities>()
            .init_resource::<RuntimeSolids>()
            .init_resource::<entities_runtime::DropIdCounter>()
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
                    entities_runtime::despawn_drops_when_dirty
                        .before(entities_runtime::reconcile_entities),
                    player::sync_mode,
                    interactive_blocks::reset_onoff_state,
                    entities_runtime::animate_kit,
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked)
                    .after(ui_bridge::update_input_capture),
            )
            .add_systems(
                Update,
                rendering::reconcile_block_overlays
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked)
                    .after(rendering::rebuild_dirty_chunks),
            )
            .configure_sets(
                Update,
                (
                    InteractionSet::MoveWorld,
                    InteractionSet::PlayerMotion.run_if(in_play),
                    InteractionSet::Detect.run_if(in_play),
                    InteractionSet::Resolve.run_if(in_play),
                    InteractionSet::SyncCollision.run_if(in_play),
                    InteractionSet::Feedback.run_if(in_play),
                )
                    .chain()
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked),
            )
            .add_systems(
                Update,
                (
                    // 1. MoveWorld: tracks, drift plates, prowler patrols.
                    entities_runtime::tick_drift_plates
                        .in_set(InteractionSet::MoveWorld)
                        .before(PhysicsSet::Writeback),
                    entities_runtime::tick_track_followers.in_set(InteractionSet::MoveWorld),
                    entities_runtime::move_prowlers.in_set(InteractionSet::MoveWorld),
                    interactive_blocks::sync_pulse.in_set(InteractionSet::MoveWorld),
                    // 2. PlayerMotion.
                    player::player_controller.in_set(InteractionSet::PlayerMotion),
                    // 3. Detect: latch roll, contacts, use target, damage.
                    interaction::begin_interaction_frame.in_set(InteractionSet::Detect),
                    interaction::gather_use_targets
                        .in_set(InteractionSet::Detect)
                        .after(interaction::begin_interaction_frame)
                        .after(ui_bridge::update_input_capture),
                    interaction::detect_contacts
                        .in_set(InteractionSet::Detect)
                        .after(interaction::begin_interaction_frame),
                    interaction::detect_damage
                        .in_set(InteractionSet::Detect)
                        .after(interaction::begin_interaction_frame),
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked),
            )
            .add_systems(
                Update,
                (
                    // 4. Resolve: use, forced motion, gates, pickups, damage.
                    interaction::resolve_use.in_set(InteractionSet::Resolve),
                    interaction::resolve_launch_pads.in_set(InteractionSet::Resolve),
                    interaction::resolve_bumpers.in_set(InteractionSet::Resolve),
                    interaction::resolve_cannons.in_set(InteractionSet::Resolve),
                    interaction::resolve_teleporters.in_set(InteractionSet::Resolve),
                    interaction::play_hazard_goal.in_set(InteractionSet::Resolve),
                    interactive_blocks::touch_onoff_switches.in_set(InteractionSet::Resolve),
                    entities_runtime::update_crumble_plates.in_set(InteractionSet::Resolve),
                    entities_runtime::update_lock_gates.in_set(InteractionSet::Resolve),
                    entities_runtime::update_relay_gates.in_set(InteractionSet::Resolve),
                    entities_runtime::touch_speed_rings.in_set(InteractionSet::Resolve),
                    entities_runtime::touch_checkpoints.in_set(InteractionSet::Resolve),
                    entities_runtime::collect_glimmers.in_set(InteractionSet::Resolve),
                    entities_runtime::collect_dropped_glimmers.in_set(InteractionSet::Resolve),
                    entities_runtime::collect_keys.in_set(InteractionSet::Resolve),
                    entities_runtime::collect_heal_orbs.in_set(InteractionSet::Resolve),
                    interaction::resolve_damage
                        .in_set(InteractionSet::Resolve)
                        .after(interaction::play_hazard_goal),
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked),
            )
            .add_systems(
                Update,
                (
                    entities_runtime::update_drops.in_set(InteractionSet::Resolve),
                    entities_runtime::update_seals.in_set(InteractionSet::Resolve),
                    interaction::resolve_forced_motion
                        .in_set(InteractionSet::Resolve)
                        .after(interaction::resolve_use)
                        .after(interaction::resolve_launch_pads)
                        .after(interaction::resolve_bumpers)
                        .after(interaction::resolve_cannons)
                        .after(interaction::resolve_teleporters)
                        .after(interaction::play_hazard_goal)
                        .after(interaction::resolve_damage),
                    entities_runtime::apply_fans
                        .in_set(InteractionSet::Resolve)
                        .after(interaction::resolve_forced_motion),
                    // 5. SyncCollision: rebuild runtime solids from state.
                    entities_runtime::rebuild_runtime_solids.in_set(InteractionSet::SyncCollision),
                    // 6. Feedback: camera follows the (possibly teleported) player.
                    camera::play_camera_follow.in_set(InteractionSet::Feedback),
                )
                    .run_if(in_state(AppState::InGame))
                    .run_if(not_paused)
                    .run_if(not_blocked),
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
                    camera::frame_selection_hotkey.run_if(in_edit),
                    camera::edit_camera_control.run_if(in_edit),
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
                    editor::update_editor_cursor
                        .after(ui_bridge::update_input_capture)
                        .before(editor::update_preview_and_edit),
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
                    limits::update_level_stats,
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
                    ui.catalog = catalog::build_catalog(&storage);
                },
            )
            .add_systems(
                OnEnter(AppState::Title),
                |mut ui: ResMut<ui_bridge::MakerUi>, storage: Res<storage::LevelStorage>| {
                    ui.level_slots = storage::list_slots(&storage);
                    ui.catalog = catalog::build_catalog(&storage);
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
    mut overlays: ResMut<rendering::BlockOverlayEntities>,
    mut entities: ResMut<EntityEntities>,
) {
    for e in &q {
        commands.entity(e).despawn();
    }
    chunks.0.clear();
    water_chunks.0.clear();
    overlays.0.clear();
    entities.0.clear();
    commands.remove_resource::<rendering::MakerAssets>();
    commands.remove_resource::<entities_runtime::EntityAssets>();
}
