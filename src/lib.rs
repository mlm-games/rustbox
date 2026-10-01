mod i18n;
pub mod maker;
pub mod menus;
mod save;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::Mut;
use game_utils::i18n::LocaleResources;
use game_utils::save::SaveManager;
use glam::{IVec3, Vec3};
use maker::block::{ALL_BLOCK_SHAPES, BlockKind};
use maker::camera::{CameraRig, play_camera_follow};
use maker::commands::{CommandHistory, EditCommand};
use maker::entities_runtime::{
    DropIdCounter, EntityEntities, LinkState, apply_fans, carry_crate_riders,
    collect_dropped_glimmers, collect_glimmers, collect_heal_orbs, collect_keys,
    despawn_drops_when_dirty, draw_props, move_prowlers, rebuild_runtime_solids,
    reconcile_entities, tick_drift_plates, tick_track_followers, touch_checkpoints,
    touch_speed_rings, update_crumble_plates, update_drops, update_lock_gates, update_relay_gates,
    update_seals,
};
use maker::entity_data::{ContainedItem, EntityDataExt, EntityKind};
use maker::interaction::{DamageRequests, ForcedMotionRequests, InteractionMemory, UseSelection};
use maker::interactive_blocks::{
    OnOffState, PulseClock, reset_onoff_state, reset_pulse_clock, sync_pulse, touch_onoff_switches,
};
use maker::level::{LevelDocument, raycast_present};
use maker::level_file::deserialize_level;
use maker::level_view::{DEFAULT_TRACK_SPEED, LevelView, resolve_click};
use maker::limits::LevelLimits;
use maker::mode::{
    ActiveLinkChannel, BoxFillStart, BrushTab, EditorClipboard, EditorCursor, InputCapture,
    MakerMode, MirrorMode, PastePreview, PlaceYaw, SelectedEntity, SelectedEntityKind,
    SelectionBoxStart, SelectionSet,
};
use maker::online::LevelMeta;
use maker::player::{
    MoveTuning, PlayIntent, PlayKeys, Player, PlayerTransform, PressedLatch, Trauma,
    clear_pressed_latch, latch_play_presses, player_controller, spawn_player, sync_mode,
};
use maker::props::RuntimeSolids;
use maker::track::{ActiveTrack, TrackMode};
use maker::{Paused, edit_ops, gizmos, interaction, not_paused, rapier, storage, win};
use repame_shell::{Sim, Staging};
use repame_view3d::{BatchDesc, Frame3d, GeomHandle, OrbitCamera, View3dEvent, Viewport3d};
use repose_core::input::{
    Key, KeyEvent, KeyEventType, PhysicalKey, PointerButton, PointerEvent, PointerEventKind,
};
use repose_core::{
    CursorIcon, FocusRequester, Modifier, RenderContext, Scheduler, View, remember, request_frame,
};
use repose_ui::{ViewExt, ZStack};
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const BUNDLED_LEVEL: &str = include_str!("../assets/levels/01_first_steps.ron");
const PLACE_REPEAT: Duration = Duration::from_millis(100);

struct App {
    sim: Rc<RefCell<Sim>>,
    cam: Rc<Cell<OrbitCamera>>,
    world: Rc<RefCell<LevelView>>,
    button: Rc<Cell<Option<PointerButton>>>,
    primary_held: Rc<Cell<bool>>,
    secondary_held: Rc<Cell<bool>>,
    place_repeat: Instant,
    ctrl: Rc<Cell<bool>>,
    shift: Rc<Cell<bool>>,
    staging: Rc<RefCell<Staging>>,
    player: Entity,
    prev_mode: MakerMode,
    looked: Rc<Cell<bool>>,
    last: Instant,
    menu_actions: menus::ActionQueue,
    menus: menus::MenuState,
    online: maker::online::OnlineRuntime,
    save_manager: SaveManager,
    save: save::SaveData,
    locale: LocaleResources,
    phase_t: f32,
    pending_mode: Option<MakerMode>,
    refocus_root: bool,
}

impl App {
    fn new() -> Self {
        let data = deserialize_level(BUNDLED_LEVEL).expect("bundled level parses");
        let mut level = LevelDocument::default();
        level.replace_data(data);
        let world = Rc::new(RefCell::new(LevelView::new(&level)));
        let mut sim = Sim::with_default_step();
        repame_rapier3d::init_world(&mut sim.world, Vec3::new(0.0, -9.81, 0.0), 1.0);
        let player = spawn_player(&mut sim.world, &level);
        sim.world.insert_resource(level);
        sim.world.insert_resource(MakerMode::Edit);
        sim.world.insert_resource(InputCapture::default());
        sim.world.insert_resource(EditorCursor::default());
        sim.world.insert_resource(MirrorMode::default());
        sim.world.insert_resource(PlayIntent::default());
        sim.world.insert_resource(PressedLatch::default());
        sim.world.insert_resource(MoveTuning::default());
        sim.world.insert_resource(RuntimeSolids::default());
        sim.world.insert_resource(OnOffState::default());
        sim.world.insert_resource(Trauma::default());
        sim.world.insert_resource(EntityEntities::default());
        sim.world.insert_resource(DropIdCounter::default());
        sim.world.insert_resource(CameraRig::default());
        sim.world.insert_resource(Paused(false));
        sim.world.insert_resource(win::MakerUi::default());
        sim.world.insert_resource(storage::LevelStorage::default());
        let mut campaign = maker::campaign::CampaignProgress::default();
        {
            let store = sim.world.resource::<storage::LevelStorage>();
            maker::campaign::load_campaign_progress(store, &mut campaign);
        }
        sim.world
            .insert_resource(maker::campaign::LevelSource::Editor);
        sim.world.insert_resource(campaign);
        sim.world.insert_resource(PulseClock::default());
        sim.world.insert_resource(InteractionMemory::default());
        sim.world.insert_resource(ForcedMotionRequests::default());
        sim.world.insert_resource(UseSelection::default());
        sim.world.insert_resource(DamageRequests::default());
        sim.world.insert_resource(LinkState::default());
        sim.world.insert_resource(BrushTab::default());
        sim.world.insert_resource(BoxFillStart::default());
        sim.world.insert_resource(SelectionSet::default());
        sim.world.insert_resource(SelectionBoxStart::default());
        sim.world.insert_resource(EditorClipboard::default());
        sim.world.insert_resource(PastePreview::default());
        sim.world.insert_resource(SelectedEntity::default());
        sim.world.insert_resource(SelectedEntityKind::default());
        sim.world.insert_resource(PlaceYaw::default());
        sim.world.insert_resource(ActiveLinkChannel::default());
        sim.world.insert_resource(ActiveTrack::default());
        sim.world.insert_resource(LevelLimits::default());
        sim.world
            .insert_resource(maker::limits::LevelStats::default());
        sim.add_chained_systems(
            (
                sync_mode,
                despawn_drops_when_dirty,
                reconcile_entities,
                reset_onoff_state,
                reset_pulse_clock,
                tick_drift_plates,
                tick_track_followers,
                move_prowlers,
                carry_crate_riders,
                rebuild_runtime_solids,
                apply_fans,
                player_controller,
                clear_pressed_latch,
            )
                .chain()
                .run_if(not_paused),
        );
        repame_rapier3d::register_rapier3d_systems(&mut sim);
        let save_manager = SaveManager::new("com", "mlm-games", "rustbox", "save.ron", 1);
        let save = save_manager.load::<save::SaveData>().sanitized();
        let mut locale = i18n::build_locale();
        if !locale.set_locale(&save.settings.language) {
            locale.set_locale("en");
        }
        Self {
            sim: Rc::new(RefCell::new(sim)),
            cam: Rc::new(Cell::new(OrbitCamera {
                target: Vec3::new(0.5, 1.0, 0.0),
                yaw: -0.7,
                pitch: 0.62,
                dist: 16.0,
                fov_y_deg: 45.0,
            })),
            world,
            button: Rc::new(Cell::new(None)),
            primary_held: Rc::new(Cell::new(false)),
            secondary_held: Rc::new(Cell::new(false)),
            place_repeat: Instant::now(),
            ctrl: Rc::new(Cell::new(false)),
            shift: Rc::new(Cell::new(false)),
            staging: Staging::shared(),
            player,
            prev_mode: MakerMode::Edit,
            looked: Rc::new(Cell::new(false)),
            last: Instant::now(),
            menu_actions: Default::default(),
            online: Default::default(),
            menus: Default::default(),
            save_manager,
            save,
            locale,
            phase_t: 0.0,
            pending_mode: None,
            refocus_root: false,
        }
    }

    fn view(&mut self, sched: &mut Scheduler, ctx: &RenderContext) -> View {
        request_frame();
        let now = Instant::now();
        let dt = now
            .duration_since(self.last)
            .min(Duration::from_secs_f32(0.25));
        self.last = now;

        let edges = {
            let mut staging = self.staging.borrow_mut();
            staging.feed_polled(sched);
            staging.take_edges()
        };
        self.ctrl.set(
            sched.held_keys.contains(&PhysicalKey::ControlLeft)
                || sched.held_keys.contains(&PhysicalKey::ControlRight),
        );
        self.shift.set(
            sched.held_keys.contains(&PhysicalKey::ShiftLeft)
                || sched.held_keys.contains(&PhysicalKey::ShiftRight),
        );

        self.drain_menu_actions(&edges);
        self.pump_online();
        self.tick_phase(dt.as_secs_f32());
        self.update_input_capture();

        let mut cam = self.cam.get();
        self.sample_input(sched, &edges);
        self.toggle_mode(&edges);
        let mut sim = self.sim.borrow_mut();
        let mode = *sim.world.resource::<MakerMode>();
        if self.prev_mode != mode {
            if (self.prev_mode, mode) == (MakerMode::Edit, MakerMode::Play) {
                let mut rig = sim.world.resource_mut::<CameraRig>();
                rig.focus = cam.target;
                rig.yaw = std::f32::consts::FRAC_PI_2 - cam.yaw;
                rig.pitch = cam.pitch;
                rig.distance = cam.dist.clamp(5.0, 22.0);
            }
            rapier::release_held_on_mode_change(&mut sim.world);
            win::on_mode_changed(&mut sim.world, self.prev_mode, mode);
            sim.world.resource_mut::<BoxFillStart>().start = None;
            if let Ok(mut world) = self.world.try_borrow_mut() {
                let mut level = sim.world.resource_mut::<LevelDocument>();
                world.release_stroke(&mut level);
            }
            self.prev_mode = mode;
        }
        win::retry_play(&mut sim.world, mode);
        let entities_rebuilt = sim.world.resource::<LevelDocument>().entities_dirty;
        sim.step(dt);
        rapier::write_back_bodies(&mut sim.world);
        rapier::move_held_objects(&mut sim.world);
        rapier::pickup_throwables(&mut sim.world);
        let dt_secs = dt.as_secs_f32();
        sync_pulse(&mut sim.world, dt_secs);
        interaction::begin_interaction_frame(&mut sim.world, entities_rebuilt);
        interaction::gather_use_targets(&mut sim.world);
        interaction::detect_contacts(&mut sim.world);
        interaction::detect_damage(&mut sim.world);
        interaction::resolve_use(&mut sim.world, dt_secs);
        interaction::resolve_launch_pads(&mut sim.world);
        interaction::resolve_bumpers(&mut sim.world);
        interaction::resolve_cannons(&mut sim.world);
        interaction::resolve_teleporters(&mut sim.world);
        interaction::play_hazard_goal(&mut sim.world);
        touch_onoff_switches(&mut sim.world);
        update_crumble_plates(&mut sim.world, dt_secs);
        update_lock_gates(&mut sim.world, dt_secs);
        update_relay_gates(&mut sim.world);
        touch_speed_rings(&mut sim.world);
        touch_checkpoints(&mut sim.world);
        collect_glimmers(&mut sim.world);
        collect_dropped_glimmers(&mut sim.world);
        collect_keys(&mut sim.world);
        collect_heal_orbs(&mut sim.world);
        interaction::resolve_damage(&mut sim.world);
        update_drops(&mut sim.world, dt_secs);
        update_seals(&mut sim.world);
        interaction::resolve_forced_motion(&mut sim.world);
        if mode == MakerMode::Play && !sim.world.resource::<Paused>().0 {
            rebuild_runtime_solids(&mut sim.world);
        }
        let looking = self.looked.replace(false);
        play_camera_follow(&mut sim.world, dt_secs, looking, &mut cam);
        win::tick_play_timer(&mut sim.world, mode, dt_secs);
        win::detect_goal(&mut sim.world, mode);
        win::tick_status(&mut sim.world, dt_secs);
        maker::limits::update_level_stats(&mut sim.world);
        if mode == MakerMode::Play {
            self.cam.set(cam);
        }
        let paused = sim.world.resource::<Paused>().0;
        sched.cursor_override =
            if mode == MakerMode::Play && !paused && self.menus.phase == menus::AppState::InGame {
                Some(CursorIcon::Hidden)
            } else {
                None
            };
        let in_edit = mode == MakerMode::Edit;
        let mut preview_cell = None;
        if in_edit {
            edit_ops::validate_refs(&mut sim.world);
        }
        let (edit_tab, paste_on, limits, shift) = if in_edit {
            (
                *sim.world.resource::<BrushTab>(),
                sim.world.resource::<PastePreview>().active,
                *sim.world.resource::<LevelLimits>(),
                self.shift.get(),
            )
        } else {
            (BrushTab::default(), false, LevelLimits::default(), false)
        };
        if in_edit {
            let ui_wants_pointer = sim.world.resource::<InputCapture>().ui_wants_pointer;
            let ui_wants_keyboard = sim.world.resource::<InputCapture>().ui_wants_keyboard;
            let ray = if ui_wants_pointer {
                None
            } else {
                sim.world.resource::<EditorCursor>().ray
            };
            let hit = ray.and_then(|(eye, dir)| {
                raycast_present(
                    sim.world.resource::<LevelDocument>(),
                    Vec3::from_array(eye),
                    Vec3::from_array(dir),
                    200.0,
                )
            });
            {
                let mut cursor = sim.world.resource_mut::<EditorCursor>();
                cursor.hit = hit.map(|(cell, _)| cell);
                cursor.place = hit.map(|(cell, normal)| cell + normal);
                preview_cell = cursor.place;
            }
            if paste_on
                && !ui_wants_pointer
                && !ui_wants_keyboard
                && let Some(target) = preview_cell.or(hit.map(|(cell, _)| cell))
            {
                sim.world.resource_mut::<PastePreview>().current_pivot = target;
            }
            if self.primary_held.get()
                && !self.secondary_held.get()
                && !self.ctrl.get()
                && !shift
                && !paste_on
                && edit_tab == BrushTab::Blocks
                && let Some(cell) = preview_cell
                && self.place_repeat.elapsed() >= PLACE_REPEAT
            {
                self.place_repeat = Instant::now();
                if let Ok(mut world) = self.world.try_borrow_mut() {
                    let mirror = sim.world.resource::<MirrorMode>().0;
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    world.stroke_paint(&mut level, mirror, cell, &limits);
                }
            }
            if !self.primary_held.get()
                && !self.secondary_held.get()
                && let Ok(mut world) = self.world.try_borrow_mut()
            {
                let mut level = sim.world.resource_mut::<LevelDocument>();
                world.release_stroke(&mut level);
            }
            if !ui_wants_keyboard {
                let held = |key: PhysicalKey| sched.held_keys.contains(&key);
                let yaw = cam.yaw;
                let forward = Vec3::new(-yaw.cos(), 0.0, -yaw.sin());
                let right = Vec3::new(yaw.sin(), 0.0, -yaw.cos());
                let mut pan = Vec3::ZERO;
                if held(PhysicalKey::KeyW) {
                    pan += forward;
                }
                if held(PhysicalKey::KeyS) {
                    pan -= forward;
                }
                if held(PhysicalKey::KeyD) {
                    pan += right;
                }
                if held(PhysicalKey::KeyA) {
                    pan -= right;
                }
                if held(PhysicalKey::KeyE) {
                    pan += Vec3::Y;
                }
                if held(PhysicalKey::KeyQ) {
                    pan -= Vec3::Y;
                }
                cam.target += pan * 12.0 * dt_secs;
                if let Some((min, max)) = sim.world.resource::<LevelDocument>().content_bounds() {
                    const PAD: f32 = 4.0;
                    let (min, max) = (min.as_vec3(), max.as_vec3() + Vec3::ONE);
                    cam.target.x = cam.target.x.clamp(min.x - PAD, max.x + PAD);
                    cam.target.z = cam.target.z.clamp(min.z - PAD, max.z + PAD);
                }
                self.cam.set(cam);
            }
        } else {
            *sim.world.resource_mut::<EditorCursor>() = EditorCursor::default();
        }
        let gizmo_groups = if in_edit {
            gizmos::edit_groups(
                sim.world.resource::<LevelDocument>(),
                sim.world.resource::<EditorCursor>().place,
                edit_tab,
                sim.world.resource::<BoxFillStart>(),
                sim.world.resource::<SelectionSet>(),
                sim.world.resource::<SelectedEntity>(),
                sim.world.resource::<PastePreview>(),
                sim.world.resource::<ActiveTrack>(),
                cam.eye(),
            )
        } else {
            Vec::new()
        };
        let mut level = sim.world.resource_mut::<LevelDocument>();
        let mut world = self.world.borrow_mut();
        world.tick(&mut level, cam.target);
        world.pump_ghosts(dt_secs);
        let mut frame = Frame3d {
            cam,
            ..Frame3d::default()
        };
        frame.push(ground());
        world.draw(
            &mut frame,
            &level,
            preview_cell.filter(|_| !paste_on),
            in_edit,
        );
        for group in gizmo_groups {
            frame.push(group);
        }

        drop(world);
        drop(level);
        if mode == MakerMode::Play
            && let (Some(p), Some(tf)) = (
                sim.world.get::<Player>(self.player),
                sim.world.get::<PlayerTransform>(self.player),
            )
            && tf.visible
        {
            frame.push(player_box(p, tf));
        }
        draw_props(&sim.world, &mut frame);
        drop(sim);

        self.hotkeys(sched, &edges);

        let cam_rc = self.cam.clone();
        let world_rc = self.world.clone();
        let sim_rc = self.sim.clone();
        let button_rc = self.button.clone();
        let secondary_rc = self.secondary_held.clone();
        let ctrl_rc = self.ctrl.clone();
        let shift_rc = self.shift.clone();
        let looked_rc = self.looked.clone();
        let viewport = Viewport3d(
            frame,
            GeomHandle::new(),
            "scene.main",
            BatchDesc::default(),
            move |ev| {
                let mut c = cam_rc.get();
                let (play_live, in_edit, ui_wants_pointer, ctrl) = {
                    let Ok(sim) = sim_rc.try_borrow() else {
                        return;
                    };
                    let m = *sim.world.resource::<MakerMode>();
                    let paused = sim.world.resource::<Paused>().0;
                    let ui_wants_pointer = sim.world.resource::<InputCapture>().ui_wants_pointer;
                    (
                        m == MakerMode::Play && !paused,
                        m == MakerMode::Edit,
                        ui_wants_pointer,
                        ctrl_rc.get(),
                    )
                };
                match ev {
                    View3dEvent::Orbit { dx, dy } => {
                        if play_live {
                            if dx * dx + dy * dy > 0.01 {
                                let Ok(mut sim) = sim_rc.try_borrow_mut() else {
                                    return;
                                };
                                let mut rig = sim.world.resource_mut::<CameraRig>();
                                rig.yaw -= dx * rig.look_sensitivity;
                                rig.pitch =
                                    (rig.pitch + dy * rig.look_sensitivity).clamp(0.08, 1.25);
                                looked_rc.set(true);
                            }
                        }
                    }
                    View3dEvent::Pan { .. } => {}
                    View3dEvent::Zoom { factor } => {
                        if play_live {
                            let scroll = (1.0 - factor) / 0.002;
                            let Ok(mut sim) = sim_rc.try_borrow_mut() else {
                                return;
                            };
                            let mut rig = sim.world.resource_mut::<CameraRig>();
                            rig.distance = (rig.distance - scroll * 0.9).clamp(5.0, 22.0);
                        } else if in_edit && !ui_wants_pointer {
                            let scroll = (1.0 - factor) / 0.002;
                            c.dist = (c.dist - scroll * 1.5).clamp(4.0, 60.0);
                        }
                    }
                    View3dEvent::HoverRay { eye, dir } => {
                        if in_edit {
                            if let Ok(mut sim) = sim_rc.try_borrow_mut() {
                                sim.world.resource_mut::<EditorCursor>().ray = Some((eye, dir));
                            }
                        }
                    }
                    View3dEvent::Hover { .. } | View3dEvent::HoverMesh { .. } => {}
                    View3dEvent::Drag { dx, dy, .. } => {
                        if in_edit && !ui_wants_pointer && !ctrl && secondary_rc.get() {
                            c.yaw += dx * 0.005;
                            c.pitch = (c.pitch + dy * 0.005).clamp(0.05, 1.5);
                        }
                    }
                    click @ (View3dEvent::GroundClick { .. } | View3dEvent::MeshClick { .. }) => {
                        let button = button_rc.get();
                        let erase = matches!(button, Some(PointerButton::Secondary));
                        let place = matches!(button, Some(PointerButton::Primary));
                        let pick = matches!(button, Some(PointerButton::Tertiary));
                        if in_edit && !ui_wants_pointer && (erase || place || pick) {
                            let Ok(mut sim) = sim_rc.try_borrow_mut() else {
                                return;
                            };
                            let paste_on = sim.world.resource::<PastePreview>().active;
                            let tab = *sim.world.resource::<BrushTab>();
                            let limits = *sim.world.resource::<LevelLimits>();
                            let mirror = sim.world.resource::<MirrorMode>().0;
                            let shift = shift_rc.get();
                            let kb_ok = !sim.world.resource::<InputCapture>().ui_wants_keyboard;
                            if paste_on {
                                if place && kb_ok {
                                    let (clipboard, pivot, yaw) = {
                                        let pv = sim.world.resource::<PastePreview>();
                                        (pv.clipboard.clone(), pv.current_pivot, pv.yaw)
                                    };
                                    let mut sel = sim.world.resource::<SelectionSet>().clone();
                                    let mut sel_ent =
                                        SelectedEntity(sim.world.resource::<SelectedEntity>().0);
                                    let Ok(mut world) = world_rc.try_borrow_mut() else {
                                        return;
                                    };
                                    let count = {
                                        let mut level = sim.world.resource_mut::<LevelDocument>();
                                        world.paste_clipboard(
                                            &mut level,
                                            &mut sel,
                                            &mut sel_ent,
                                            &clipboard,
                                            pivot,
                                            yaw,
                                            &limits,
                                        )
                                    };
                                    *sim.world.resource_mut::<SelectionSet>() = sel;
                                    sim.world.resource_mut::<SelectedEntity>().0 = sel_ent.0;
                                    sim.world.resource_mut::<PastePreview>().active = false;
                                    let msg = format!("Pasted {count} item(s)");
                                    sim.world
                                        .resource_mut::<win::MakerUi>()
                                        .set_status(msg.clone());
                                    world.last_action = msg;
                                } else if erase && kb_ok {
                                    sim.world.resource_mut::<PastePreview>().reset();
                                    sim.world.resource_mut::<SelectionBoxStart>().start = None;
                                    let msg = "Paste preview cancelled".to_string();
                                    sim.world
                                        .resource_mut::<win::MakerUi>()
                                        .set_status(msg.clone());
                                    if let Ok(mut world) = world_rc.try_borrow_mut() {
                                        world.last_action = msg;
                                    }
                                }
                            } else if ctrl {
                                if place && kb_ok {
                                    let resolved = {
                                        let level = sim.world.resource::<LevelDocument>();
                                        resolve_click(&level, &click, &c)
                                    };
                                    let Some((hit, _normal)) = resolved else {
                                        return;
                                    };
                                    if !shift {
                                        sim.world.resource_mut::<SelectionSet>().clear();
                                        sim.world.resource_mut::<SelectedEntity>().0 = None;
                                    }
                                    let hit_entity = {
                                        let level = sim.world.resource::<LevelDocument>();
                                        level.top_entity_at_cell(hit).map(|e| e.id)
                                    };
                                    let has_block = {
                                        let level = sim.world.resource::<LevelDocument>();
                                        level.get_block(hit).is_some()
                                    };
                                    if let Some(id) = hit_entity {
                                        sim.world.resource_mut::<SelectionSet>().toggle_entity(id);
                                        sim.world.resource_mut::<SelectedEntity>().0 = Some(id);
                                    } else if has_block {
                                        sim.world.resource_mut::<SelectionSet>().toggle_block(hit);
                                        sim.world.resource_mut::<SelectedEntity>().0 = None;
                                    }
                                    let count = sim.world.resource::<SelectionSet>().len();
                                    let msg = format!("Selected {count} item(s)");
                                    sim.world
                                        .resource_mut::<win::MakerUi>()
                                        .set_status(msg.clone());
                                    if let Ok(mut world) = world_rc.try_borrow_mut() {
                                        world.last_action = msg;
                                    }
                                }
                            } else if pick {
                                let resolved = {
                                    let level = sim.world.resource::<LevelDocument>();
                                    resolve_click(&level, &click, &c)
                                };
                                let Some((hit, _)) = resolved else {
                                    return;
                                };
                                let block = {
                                    let level = sim.world.resource::<LevelDocument>();
                                    level.get_block(hit).cloned()
                                };
                                let entity_kind = {
                                    let level = sim.world.resource::<LevelDocument>();
                                    level.top_entity_at_cell(hit).map(|e| e.kind)
                                };
                                if let Some(b) = block {
                                    let Ok(mut world) = world_rc.try_borrow_mut() else {
                                        return;
                                    };
                                    world.brush.kind = b.kind;
                                    world.brush.shape = b.shape;
                                    world.brush.rot = b.rot & 3;
                                    world.brush.waterlogged = b.waterlogged;
                                    *sim.world.resource_mut::<BrushTab>() = BrushTab::Blocks;
                                    world.last_action = format!("pick {:?}", b.kind);
                                } else if let Some(kind) = entity_kind {
                                    sim.world.resource_mut::<SelectedEntityKind>().0 = kind;
                                    *sim.world.resource_mut::<BrushTab>() = BrushTab::Entities;
                                }
                            } else {
                                let resolved = {
                                    let level = sim.world.resource::<LevelDocument>();
                                    resolve_click(&level, &click, &c)
                                };
                                let Some((hit, normal)) = resolved else {
                                    return;
                                };
                                let place_cell = hit + normal;
                                let Ok(mut world) = world_rc.try_borrow_mut() else {
                                    return;
                                };
                                if place && tab == BrushTab::Blocks && shift {
                                    let start = sim.world.resource::<BoxFillStart>().start;
                                    match start {
                                        None => {
                                            sim.world.resource_mut::<BoxFillStart>().start =
                                                Some(place_cell);
                                        }
                                        Some(a) => {
                                            sim.world.resource_mut::<BoxFillStart>().start = None;
                                            let count = {
                                                let mut level =
                                                    sim.world.resource_mut::<LevelDocument>();
                                                world.box_fill(&mut level, a, place_cell, &limits)
                                            };
                                            if count == 0 {
                                                world.last_action = "box fill skipped".to_string();
                                            }
                                        }
                                    }
                                } else if place && tab == BrushTab::Blocks {
                                    if world.stroke_click_ready() {
                                        let mut level = sim.world.resource_mut::<LevelDocument>();
                                        world.click_place(&mut level, mirror, place_cell, &limits);
                                    }
                                } else if place && tab == BrushTab::Entities {
                                    let kind = sim.world.resource::<SelectedEntityKind>().0;
                                    let yaw = sim.world.resource::<PlaceYaw>().0;
                                    let channel = sim.world.resource::<ActiveLinkChannel>().0;
                                    let placed = {
                                        let mut level = sim.world.resource_mut::<LevelDocument>();
                                        world.place_entity(
                                            &mut level, place_cell, kind, yaw, channel, &limits,
                                        )
                                    };
                                    if !placed {
                                        world.last_action = "can't place entity here".to_string();
                                    }
                                } else if place && tab == BrushTab::Tracks {
                                    let mut active = *sim.world.resource::<ActiveTrack>();
                                    {
                                        let mut level = sim.world.resource_mut::<LevelDocument>();
                                        world.track_place_click(
                                            &mut level,
                                            place_cell,
                                            &mut active,
                                            &limits,
                                        );
                                    }
                                    *sim.world.resource_mut::<ActiveTrack>() = active;
                                } else if erase && tab == BrushTab::Tracks {
                                    let mut active = *sim.world.resource::<ActiveTrack>();
                                    {
                                        let mut level = sim.world.resource_mut::<LevelDocument>();
                                        world.track_erase_click(
                                            &mut level,
                                            place_cell,
                                            &mut active,
                                        );
                                    }
                                    *sim.world.resource_mut::<ActiveTrack>() = active;
                                } else if erase && world.stroke_click_ready() {
                                    let mut sel_ent =
                                        SelectedEntity(sim.world.resource::<SelectedEntity>().0);
                                    {
                                        let mut level = sim.world.resource_mut::<LevelDocument>();
                                        world.erase_at(&mut level, mirror, hit, &mut sel_ent);
                                    }
                                    sim.world.resource_mut::<SelectedEntity>().0 = sel_ent.0;
                                }
                            }
                        }
                    }
                }
                cam_rc.set(c);
            },
        );

        let button = self.button.clone();
        let button_up = self.button.clone();
        let button_cancel = self.button.clone();
        let primary_down = self.primary_held.clone();
        let secondary_down = self.secondary_held.clone();
        let primary_up = self.primary_held.clone();
        let secondary_up = self.secondary_held.clone();
        let primary_cancel = self.primary_held.clone();
        let secondary_cancel = self.secondary_held.clone();
        // Pointer state tracks the 3D view only: presses that land on HUD
        // buttons never reach this wrapper (its hit region is not in their
        // path), so UI clicks cannot place or erase world blocks.
        let viewport = ZStack(
            Modifier::new()
                .fill_max_size()
                .on_pointer_down(move |ev: PointerEvent| {
                    if let PointerEventKind::Down(b) = ev.event {
                        button.set(Some(b));
                        match b {
                            PointerButton::Primary => primary_down.set(true),
                            PointerButton::Secondary => secondary_down.set(true),
                            _ => {}
                        }
                    }
                })
                .on_pointer_up(move |ev: PointerEvent| {
                    if let PointerEventKind::Up(b) = ev.event {
                        button_up.set(None);
                        match b {
                            PointerButton::Primary => primary_up.set(false),
                            PointerButton::Secondary => secondary_up.set(false),
                            _ => {}
                        }
                    }
                })
                .on_pointer_cancel(move |_| {
                    button_cancel.set(None);
                    primary_cancel.set(false);
                    secondary_cancel.set(false);
                }),
        )
        .child(viewport);

        let focus = remember(FocusRequester::new);
        let fr_positioned = (*focus).clone();
        if self.refocus_root {
            self.refocus_root = false;
            (*focus).request_focus();
        }
        let focus_staging = self.staging.clone();
        let key_staging = self.staging.clone();
        ZStack(
            Modifier::new()
                .fill_max_size()
                .focusable(true)
                .focus_requester((*focus).clone())
                .on_globally_positioned(move |_| {
                    fr_positioned.request_focus();
                })
                .on_focus_changed(move |focused| {
                    focus_staging.borrow_mut().set_window_focused(focused);
                })
                .on_key_event(move |ke: KeyEvent| {
                    let mut staging = key_staging.borrow_mut();
                    let down = matches!(ke.event_type, KeyEventType::Down);
                    match ke.key {
                        Key::Escape | Key::Enter | Key::Delete => {
                            let physical = match ke.key {
                                Key::Escape => PhysicalKey::Escape,
                                Key::Enter => PhysicalKey::Enter,
                                _ => PhysicalKey::Delete,
                            };
                            staging.stage_physical(physical, down, ke.is_repeat);
                        }
                        _ => staging.handle_key(&ke),
                    }
                    false
                }),
        )
        .child({
            if self.menus.phase == menus::AppState::InGame
                && self.menus.block_icon_handles.is_empty()
            {
                self.menus.block_icon_handles = menus::icons::register_block_icons(ctx);
                self.menus.entity_icon_handles = menus::icons::register_entity_icons(ctx);
            }
            self.sync_menu_state();
            let mut layer: Vec<View> = Vec::new();
            if self.menus.phase == menus::AppState::InGame {
                layer.push(viewport);
            }
            layer.push(menus::compose_root(&self.menus, self.menu_actions.clone()));
            layer
        })
    }

    fn sample_input(&mut self, sched: &Scheduler, edges: &[PhysicalKey]) {
        let mut sim = self.sim.borrow_mut();
        let kb_ok = !sim.world.resource::<InputCapture>().ui_wants_keyboard;
        let keys = PlayKeys {
            held: &sched.held_keys,
            edges,
            kb_ok,
        };
        let input = keys.read_play_input();
        if *sim.world.resource::<MakerMode>() == MakerMode::Play
            && !sim.world.resource::<Paused>().0
        {
            let mut latch = sim.world.resource_mut::<PressedLatch>();
            latch_play_presses(&input, &mut latch);
        }
        *sim.world.resource_mut::<PlayIntent>() = input;
    }

    fn toggle_mode(&mut self, edges: &[PhysicalKey]) {
        let mut sim = self.sim.borrow_mut();
        if sim.world.resource::<Paused>().0 {
            return;
        }
        if sim.world.resource::<InputCapture>().ui_wants_keyboard {
            return;
        }
        if !edges.contains(&PhysicalKey::Tab) {
            return;
        }
        let mut mode = sim.world.resource_mut::<MakerMode>();
        *mode = match *mode {
            MakerMode::Edit => MakerMode::Play,
            MakerMode::Play => MakerMode::Edit,
        };
    }

    fn announce(&self, msg: impl Into<String>) {
        let msg = msg.into();
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            sim.world
                .resource_mut::<win::MakerUi>()
                .set_status(msg.clone());
        }
        if let Ok(mut world) = self.world.try_borrow_mut() {
            world.last_action = msg;
        }
    }

    fn pump_online(&mut self) {
        let mut notes: Vec<String> = Vec::new();

        // Creator identity: created on first run, replaced by a recovery-key
        // import; copied into the request config so every call carries it.
        if self.menus.creator_recovery_key.is_empty() {
            let res = {
                let Ok(sim) = self.sim.try_borrow() else {
                    return;
                };
                let store = sim.world.resource::<storage::LevelStorage>();
                maker::creator::load_or_create(store)
            };
            match res {
                Ok(id) => {
                    self.menus.creator_recovery_key = id.recovery_key;
                    self.menus.creator_device_id = id.device_id;
                }
                Err(e) => notes.push(format!("Creator identity unavailable: {e}")),
            }
        }
        if !self.menus.creator_import_code.trim().is_empty() {
            let code = self.menus.creator_import_code.trim().to_string();
            self.menus.creator_import_code.clear();
            let res = {
                let Ok(sim) = self.sim.try_borrow() else {
                    return;
                };
                let store = sim.world.resource::<storage::LevelStorage>();
                maker::creator::import_recovery_key(store, &code)
            };
            match res {
                Ok(id) => {
                    self.menus.creator_recovery_key = id.recovery_key;
                    self.menus.creator_device_id = id.device_id;
                    notes.push("Recovery key imported, restoring your levels...".to_string());
                    self.menus
                        .online_pending
                        .push(maker::online::OnlineRequest::Me);
                    self.menus
                        .online_pending
                        .push(maker::online::OnlineRequest::MyLevels);
                }
                Err(e) => notes.push(format!("Recovery key import failed: {e}")),
            }
        }
        if self.online.config.recovery_key != self.menus.creator_recovery_key {
            self.online.config.recovery_key = self.menus.creator_recovery_key.clone();
        }
        if self.online.config.device_id != self.menus.creator_device_id {
            self.online.config.device_id = self.menus.creator_device_id.clone();
        }
        let token = self.menus.online_token.trim().to_string();
        if self.online.config.token != token {
            self.online.config.token = token;
        }

        // Dispatch queued requests; downloads answer from cache when possible
        // (plays always go to the network so `count=1` is honored).
        for req in std::mem::take(&mut self.menus.online_pending) {
            if let maker::online::OnlineRequest::Download { play, .. } = &req
                && *play
            {
                maker::online::dispatch(&self.online.config, &self.online.tx, req);
                continue;
            }
            if let maker::online::OnlineRequest::Download { meta, play } = &req {
                if let Some(data) = self.online.cache.get(&meta.id).cloned() {
                    if !play {
                        let preview = maker::thumbnail::render_preview(
                            &data,
                            maker::catalog::PREVIEW_COLS,
                            maker::catalog::PREVIEW_ROWS,
                        );
                        self.menus.online_preview_pending.retain(|x| *x != meta.id);
                        self.menus.online_previews.insert(meta.id, preview);
                        self.menus.touch_online_preview_lru(meta.id);
                    }
                    if *play {
                        self.apply_online_download(meta, data);
                    }
                    continue;
                }
            }
            maker::online::dispatch(&self.online.config, &self.online.tx, req);
        }

        // Drain completed fetch callbacks (callbacks fire on background
        // threads; applying happens here on the main thread).
        let mut events = Vec::new();
        while let Ok(ev) = self.online.rx.try_recv() {
            events.push(ev);
        }
        for ev in events {
            self.handle_online_event(ev, &mut notes);
        }

        for n in notes {
            self.announce(n);
        }
    }

    /// Swap the live document to a downloaded level and start playing it.
    fn apply_online_download(&mut self, meta: &LevelMeta, data: maker::level::LevelData) {
        let outcome = match self.sim.try_borrow_mut() {
            Ok(mut sim) => sim
                .world
                .resource_scope(|world, mut level: Mut<LevelDocument>| {
                    let Ok(mut view) = self.world.try_borrow_mut() else {
                        return Err(anyhow::anyhow!("view busy"));
                    };
                    view.release_stroke(&mut level);
                    storage::apply_level_data(&mut level, &mut view.history, data);
                    level.data.name = meta.name.clone();
                    level.data.author = meta.author.clone();
                    view.sync_source(&level);
                    world.resource_mut::<SelectedEntity>().0 = None;
                    world.resource_mut::<ActiveTrack>().0 = None;
                    *world.resource_mut::<maker::campaign::LevelSource>() =
                        maker::campaign::LevelSource::Imported;
                    let ui = &mut world.resource_mut::<win::MakerUi>();
                    ui.current_key = None;
                    ui.goal_latched = false;
                    ui.play_timer = 0.0;
                    ui.deaths = 0;
                    ui.clear_time_secs = 0.0;
                    ui.clear_deaths = 0;
                    ui.first_clear = false;
                    ui.new_record = false;
                    ui.player_is_author = false;
                    ui.clear_pending = false;
                    world.resource_mut::<Paused>().0 = false;
                    *world.resource_mut::<MakerMode>() = MakerMode::Play;
                    Ok(())
                }),
            Err(_) => return,
        };
        let _: anyhow::Result<()> = outcome;
    }

    fn handle_online_event(&mut self, ev: maker::online::OnlineEvent, notes: &mut Vec<String>) {
        use maker::online::OnlineEvent;
        match ev {
            OnlineEvent::Listed(result) => match result {
                Ok(resp) => {
                    self.menus.online_loading = false;
                    self.menus.online_total = resp.total;
                    if self.menus.online_last_offset == 0 {
                        self.menus.online_levels = resp.levels;
                    } else {
                        for m in resp.levels {
                            if !self.menus.online_levels.iter().any(|x| x.id == m.id) {
                                self.menus.online_levels.push(m);
                            }
                        }
                    }
                    self.menus.online_confirm_delete = None;
                    self.menus.sort_online_levels();
                    self.menus.reconcile_online_nav();
                    let shown = self.menus.online_levels.len() as u64;
                    let total = self.menus.online_total;
                    notes.push(if shown < total {
                        format!("Showing {shown} of {total} levels online")
                    } else {
                        format!("{total} levels online")
                    });
                }
                Err(e) => {
                    self.menus.online_loading = false;
                    notes.push(format!("Browse failed: {e}"));
                }
            },
            OnlineEvent::FetchedById { id, result } => match result {
                Ok(meta) => {
                    self.menus.online_loading = false;
                    self.menus.online_levels = vec![meta];
                    self.menus.online_total = 1;
                    self.menus.online_last_offset = 0;
                    self.menus.online_selected = Some(id);
                    self.menus.online_confirm_delete = None;
                    self.menus.sort_online_levels();
                    self.menus.reconcile_online_nav();
                    notes.push(format!("Found #{id}"));
                }
                Err(e) => {
                    self.menus.online_loading = false;
                    notes.push(format!("ID search ({id}): {e}"));
                }
            },
            OnlineEvent::Uploaded(result) => match result {
                Ok(resp) => notes.push(format!("Published as #{}", resp.id)),
                Err(e) => notes.push(format!("Upload failed: {e}")),
            },
            OnlineEvent::Downloaded { meta, result, play } => match result {
                Ok(data) => {
                    let preview = maker::thumbnail::render_preview(
                        &data,
                        maker::catalog::PREVIEW_COLS,
                        maker::catalog::PREVIEW_ROWS,
                    );
                    self.menus.online_preview_pending.retain(|x| *x != meta.id);
                    self.menus.online_previews.insert(meta.id, preview);
                    const MAX_CACHED_LEVELS: usize = 32;
                    if self.online.cache.len() >= MAX_CACHED_LEVELS
                        && !self.online.cache.contains_key(&meta.id)
                        && let Some(old) = self.online.cache.keys().next().copied()
                    {
                        self.online.cache.remove(&old);
                    }
                    self.online.cache.insert(meta.id, data.clone());
                    self.menus.touch_online_preview_lru(meta.id);
                    if play {
                        self.apply_online_download(&meta, data);
                        notes.push(format!("Downloaded & playing: {}", meta.name));
                    } else {
                        notes.push(format!("Downloaded: {}", meta.name));
                    }
                }
                Err(e) => {
                    self.menus.online_preview_pending.retain(|x| *x != meta.id);
                    notes.push(format!("Download failed: {e}"));
                }
            },
            OnlineEvent::Liked { id, result } => match result {
                Ok(()) => notes.push(format!("Liked #{id}")),
                Err(e) => notes.push(format!("Like failed: {e}")),
            },
            OnlineEvent::Reported { id, result } => match result {
                Ok(()) => notes.push(format!("Reported #{id} - thanks!")),
                Err(e) => notes.push(format!("Report failed: {e}")),
            },
            OnlineEvent::Deleted { id, result } => match result {
                Ok(()) => {
                    self.online.cache.remove(&id);
                    self.menus.online_previews.remove(&id);
                    self.menus.online_preview_pending.retain(|x| *x != id);
                    self.menus.online_preview_lru.retain(|x| *x != id);
                    self.menus.online_levels.retain(|m| m.id != id);
                    if self.menus.online_selected == Some(id) {
                        self.menus.online_selected = None;
                    }
                    self.menus.online_confirm_delete = None;
                    self.menus.reconcile_online_nav();
                    notes.push(format!("Deleted #{id}"));
                }
                Err(e) => notes.push(format!("Delete failed: {e}")),
            },
            OnlineEvent::Me(result) => match result {
                Ok(me) => {
                    let used = me.uploads_used_this_week;
                    let cap = used + me.uploads_remaining_this_week;
                    self.menus.creator_quota_text = format!("{used}/{cap} uploads used this week");
                }
                Err(e) => self.menus.creator_quota_text = format!("Quota unknown ({e})"),
            },
            OnlineEvent::MyLevels(result) => match result {
                Ok(resp) => {
                    self.menus.online_levels = resp.levels;
                    self.menus.online_total = resp.total;
                    self.menus.online_last_offset = 0;
                    self.menus.online_loading = false;
                    self.menus.online_confirm_delete = None;
                    self.menus.sort_online_levels();
                    self.menus.reconcile_online_nav();
                    notes.push(format!("{} of your levels online", resp.total));
                }
                Err(e) => {
                    self.menus.online_loading = false;
                    notes.push(format!("My levels failed: {e}"));
                }
            },
        }
    }

    fn drain_menu_actions(&mut self, edges: &[PhysicalKey]) {
        let batch = {
            let Ok(mut queue) = self.menu_actions.lock() else {
                return;
            };
            std::mem::take(&mut *queue)
        };
        let drained = !batch.is_empty();
        let prev_overlay = self.menus.overlay;
        let prev_phase = self.menus.phase;
        for action in batch {
            self.apply_menu_action(action);
        }
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            let mut ui = sim.world.resource_mut::<win::MakerUi>();
            if ui.clear_pending {
                ui.clear_pending = false;
                drop(ui);
                self.menus.overlay = menus::OverlayMenu::LevelClear;
            }
        }
        if !matches!(
            self.menus.overlay,
            menus::OverlayMenu::Browse
                | menus::OverlayMenu::Online
                | menus::OverlayMenu::Share
                | menus::OverlayMenu::LevelInfo
        ) && !self.menus.sign_editor_open
        {
            self.menus.keyboard_captured = false;
        }
        self.handle_pause_escape(edges);
        if (drained || self.menus.overlay != prev_overlay || self.menus.phase != prev_phase)
            && !matches!(
                self.menus.overlay,
                menus::OverlayMenu::Browse
                    | menus::OverlayMenu::Online
                    | menus::OverlayMenu::Share
                    | menus::OverlayMenu::LevelInfo
            )
            && !self.menus.sign_editor_open
        {
            self.refocus_root = true;
        }
    }

    fn apply_menu_action(&mut self, action: menus::UiAction) {
        use menus::OverlayMenu;
        use menus::UiAction;
        match action {
            UiAction::StartGame => {
                self.pending_mode = Some(MakerMode::Edit);
                self.begin_loading();
            }
            UiAction::CloseOverlay => {
                if self.menus.overlay == OverlayMenu::Settings {
                    let saved = self.menus.saved_language.clone();
                    self.locale.set_locale(&saved);
                }
                let paused = self.is_paused();
                self.menus.overlay = match self.menus.overlay {
                    OverlayMenu::Settings | OverlayMenu::Credits | OverlayMenu::Share if paused => {
                        OverlayMenu::Pause
                    }
                    OverlayMenu::LevelClear => {
                        self.set_paused(false);
                        self.set_mode(MakerMode::Edit);
                        OverlayMenu::None
                    }
                    _ => OverlayMenu::None,
                };
            }
            UiAction::Resume => {
                self.menus.overlay = OverlayMenu::None;
                self.set_paused(false);
            }
            UiAction::QuitToTitle => {
                self.menus.overlay = OverlayMenu::None;
                self.set_paused(false);
                self.set_mode(MakerMode::Edit);
                self.menus.phase = menus::AppState::Title;
                self.phase_t = 0.0;
            }
            UiAction::QuitApp => {
                #[cfg(not(target_arch = "wasm32"))]
                std::process::exit(0);
            }
            UiAction::OpenSettings => {
                self.menus.saved_language = self.locale.current.clone();
                self.menus.overlay = OverlayMenu::Settings;
            }
            UiAction::OpenCredits => {
                self.menus.overlay = OverlayMenu::Credits;
            }
            UiAction::OpenLevelSelect => {
                self.menus.overlay = OverlayMenu::LevelSelect;
            }
            UiAction::SetMasterVol(v) => self.menus.master_vol = v.clamp(0.0, 1.0),
            UiAction::SetSfxVol(v) => self.menus.sfx_vol = v.clamp(0.0, 1.0),
            UiAction::SetMusicVol(v) => self.menus.music_vol = v.clamp(0.0, 1.0),
            UiAction::SaveSettings => {
                self.save.settings.master_volume = self.menus.master_vol;
                self.save.settings.sfx_volume = self.menus.sfx_vol;
                self.save.settings.music_volume = self.menus.music_vol;
                self.save.settings.language = self.locale.current.clone();
                let _ = self.save_manager.save(&self.save);
                self.menus.saved_language = self.locale.current.clone();
                self.menus.overlay = if self.is_paused() {
                    OverlayMenu::Pause
                } else {
                    OverlayMenu::None
                };
            }
            UiAction::NextLanguage => {
                let next = {
                    let avail = &self.locale.available;
                    avail
                        .iter()
                        .position(|l| *l == self.locale.current)
                        .map(|i| avail[(i + 1) % avail.len()].clone())
                };
                if let Some(lang) = next {
                    self.locale.set_locale(&lang);
                }
            }
            UiAction::SetLanguage(lang) => {
                self.locale.set_locale(&lang);
            }
            UiAction::BrowseAddToCollection => {
                let bundled = {
                    let Ok(sim) = self.sim.try_borrow() else {
                        return;
                    };
                    matches!(
                        *sim.world.resource::<maker::campaign::LevelSource>(),
                        maker::campaign::LevelSource::Bundled(_)
                    )
                };
                if bundled {
                    self.announce("Remix bundled levels before saving.");
                    return;
                }
                let res = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    sim.world
                        .resource_scope(|world, mut level: Mut<LevelDocument>| {
                            let store = world.resource::<storage::LevelStorage>();
                            let res = storage::save_to_collection(store, &mut level);
                            if let Ok(key) = &res {
                                world.resource_mut::<win::MakerUi>().current_key =
                                    Some(key.clone());
                            }
                            res.map(|_| ())
                        })
                };
                match res {
                    Ok(()) => {
                        self.rebuild_catalog();
                        self.announce("Saved to your collection.");
                    }
                    Err(e) => self.announce(format!("Save failed: {e}")),
                }
            }
            UiAction::MakerPublish => {
                let outcome = {
                    let Ok(sim) = self.sim.try_borrow() else {
                        return;
                    };
                    let level = sim.world.resource::<LevelDocument>();
                    if level.data.is_verified {
                        Some(storage::export_level_code(&level.data))
                    } else {
                        None
                    }
                };
                match outcome {
                    None => self.announce("Beat the level before publishing."),
                    Some(Ok(code)) => {
                        self.menus.export_code = code;
                        self.menus.export_error = None;
                        self.menus.overlay = OverlayMenu::Share;
                    }
                    Some(Err(e)) => {
                        self.menus.export_code.clear();
                        self.menus.export_error = Some(format!("Export failed: {e}"));
                        self.menus.overlay = OverlayMenu::Share;
                    }
                }
            }
            UiAction::MakerExportCode => {
                let res = {
                    let Ok(sim) = self.sim.try_borrow() else {
                        return;
                    };
                    let level = sim.world.resource::<LevelDocument>();
                    storage::export_level_code(&level.data)
                };
                match res {
                    Ok(code) => {
                        self.menus.export_code = code;
                        self.menus.export_error = None;
                    }
                    Err(e) => {
                        self.menus.export_code.clear();
                        self.menus.export_error = Some(format!("Export failed: {e}"));
                    }
                }
            }
            UiAction::MakerCopyCode => {
                if self.menus.export_code.is_empty() {
                    self.announce("No code to copy yet.");
                } else {
                    repose_core::clipboard::copy_to_clipboard(&self.menus.export_code);
                    self.announce("Code copied!");
                }
            }
            UiAction::MakerImportCode(code) => {
                let data = match storage::import_level_code(code.trim()) {
                    Ok(d) => d,
                    Err(e) => {
                        self.announce(format!("Bad code: {e}"));
                        return;
                    }
                };
                let outcome = match self.sim.try_borrow_mut() {
                    Ok(mut sim) => {
                        sim.world
                            .resource_scope(|_world, mut level: Mut<LevelDocument>| {
                                let Ok(mut view) = self.world.try_borrow_mut() else {
                                    return Err(anyhow::anyhow!("view busy"));
                                };
                                view.release_stroke(&mut level);
                                storage::apply_level_data(&mut level, &mut view.history, data);
                                view.sync_source(&level);
                                Ok(())
                            })
                    }
                    Err(_) => return,
                };
                if outcome.is_err() {
                    return;
                }
                self.menus.export_code.clear();
                self.menus.export_error = None;
                self.menus.overlay = OverlayMenu::None;
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    sim.world.resource_mut::<win::MakerUi>().current_key = None;
                    sim.world.resource_mut::<SelectedEntity>().0 = None;
                    sim.world.resource_mut::<ActiveTrack>().0 = None;
                    *sim.world.resource_mut::<maker::campaign::LevelSource>() =
                        maker::campaign::LevelSource::Imported;
                }
                self.set_mode(MakerMode::Play);
                self.announce("Level imported!");
            }
            UiAction::LevelInfoOpen => {
                if let Ok(sim) = self.sim.try_borrow() {
                    let level = sim.world.resource::<LevelDocument>();
                    self.menus.info_name = level.data.name.clone();
                    self.menus.info_author = level.data.author.clone();
                    self.menus.info_description = level.data.description.clone();
                    self.menus.info_tags = level.data.tags.clone();
                    self.menus.info_clear_condition = level.data.clear_condition;
                    self.menus.info_preset = level.data.boundary.boundary_preset();
                    self.menus.info_water = level.data.water_level;
                    self.menus.info_size = level.play_size();
                    self.menus.info_size_auto = level.data.size.is_none();
                    self.menus.info_height = level.data.boundary.height;
                    self.menus.info_blocks = level.map.len() as u32;
                    self.menus.info_entities = level.data.entities.len() as u32;
                }
                self.menus.overlay = OverlayMenu::LevelInfo;
            }
            UiAction::LevelInfoClose => {
                self.menus.overlay = OverlayMenu::None;
            }
            UiAction::LevelInfoSave => {
                let res = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    sim.world
                        .resource_scope(|world, mut level: Mut<LevelDocument>| {
                            level.data.name = self.menus.info_name.clone();
                            level.data.author = self.menus.info_author.clone();
                            level.data.description = self.menus.info_description.clone();
                            level.data.tags = self.menus.info_tags.clone();
                            level.data.clear_condition = self.menus.info_clear_condition;
                            if level.data.created_at == 0 {
                                level.data.created_at = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_secs())
                                    .unwrap_or(0);
                            }
                            let key = world.resource::<win::MakerUi>().current_key.clone();
                            let store = world.resource::<storage::LevelStorage>();
                            match key {
                                Some(k) => storage::save_level(store, &mut level, &k),
                                None => match storage::save_to_collection(store, &mut level) {
                                    Ok(k) => {
                                        world.resource_mut::<win::MakerUi>().current_key = Some(k);
                                        Ok(())
                                    }
                                    Err(e) => Err(e),
                                },
                            }
                        })
                };
                match res {
                    Ok(()) => {
                        self.refresh_level_slots();
                        self.rebuild_catalog();
                        self.announce("Level info saved.");
                        self.menus.overlay = OverlayMenu::None;
                    }
                    Err(e) => self.announce(format!("Save failed: {e}")),
                }
            }
            UiAction::LevelInfoCycleClearCondition => {
                use maker::level::ClearCondition as Cc;
                self.menus.info_clear_condition = match self.menus.info_clear_condition {
                    Cc::ReachGoal => Cc::CollectAllGlimmers,
                    Cc::CollectAllGlimmers => Cc::DefeatAllProwlers,
                    Cc::DefeatAllProwlers => Cc::NoDeath,
                    Cc::NoDeath => Cc::TimeLimitMs(60_000),
                    Cc::TimeLimitMs(_) => Cc::ReachGoal,
                };
            }
            UiAction::LevelInfoTimeLimitDelta(d) => {
                if let maker::level::ClearCondition::TimeLimitMs(ms) =
                    &mut self.menus.info_clear_condition
                {
                    let next = (*ms as i64 + d as i64 * 1000).max(5_000);
                    *ms = next as u32;
                }
            }
            UiAction::LevelInfoToggleTag(tag) => {
                if let Some(pos) = self.menus.info_tags.iter().position(|t| *t == tag) {
                    self.menus.info_tags.remove(pos);
                } else {
                    self.menus.info_tags.push(tag);
                }
            }
            UiAction::LevelInfoSetText(idx, v) => match idx {
                0 => self.menus.info_name = v,
                1 => self.menus.info_author = v,
                _ => self.menus.info_description = v,
            },
            UiAction::LevelInfoPreset(preset) => {
                let status = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    level.data.boundary = preset.config();
                    level.mark_all_dirty();
                    format!("Boundary: {}", preset.label())
                };
                self.menus.info_preset = Some(preset);
                self.announce(status);
            }
            UiAction::LevelInfoWaterDelta(d) => {
                let next = match self.menus.info_water {
                    Some(level) if d > 0 => Some(level + d),
                    Some(level) if level + d < 0 => None,
                    Some(level) => Some((level + d).max(0)),
                    None if d > 0 => Some(1),
                    None => None,
                };
                self.menus.info_water = next;
                let status = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    level.data.water_level = next;
                    level.mark_all_dirty();
                    match next {
                        Some(y) => format!("Water plane at y={y}"),
                        None => "Water: off".to_string(),
                    }
                };
                self.announce(status);
            }
            UiAction::LevelInfoSizeDelta(d) => {
                let status = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    let base = level.data.size.unwrap_or_else(|| level.play_size());
                    let mut next = [base[0] + d, base[1] + d, base[2] + d];
                    let mut need = [1, 1, 1];
                    for b in level.map.values() {
                        need[0] = need[0].max(b.position[0].abs());
                        need[1] = need[1].max(b.position[1]);
                        need[2] = need[2].max(b.position[2].abs());
                    }
                    for i in 0..3 {
                        next[i] = next[i].max(need[i]);
                    }
                    level.data.size = Some(next);
                    level.mark_all_dirty();
                    self.menus.info_size = next;
                    self.menus.info_size_auto = false;
                    format!("Size {}×{}×{}", next[0], next[1], next[2])
                };
                self.announce(status);
            }
            UiAction::LevelInfoSizeAuto => {
                let status = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    level.data.size = None;
                    level.mark_all_dirty();
                    self.menus.info_size = level.play_size();
                    self.menus.info_size_auto = true;
                    "Size: auto (from content)".to_string()
                };
                self.announce(status);
            }
            UiAction::LevelInfoHeightDelta(d) => {
                let status = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    let h = (self.menus.info_height + d).max(0);
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    level.data.boundary.height = h;
                    level.mark_all_dirty();
                    self.menus.info_height = h;
                    if h == 0 {
                        "Wall height: auto (from size)".to_string()
                    } else {
                        format!("Wall height: {h} cells")
                    }
                };
                self.announce(status);
            }
            UiAction::LevelInfoHeightAuto => {
                let status = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    level.data.boundary.height = 0;
                    level.mark_all_dirty();
                    self.menus.info_height = 0;
                    "Wall height: auto (from size)".to_string()
                };
                self.announce(status);
            }
            UiAction::MakerRetry => {
                self.menus.overlay = menus::OverlayMenu::None;
                if let Ok(mut sim) = self.sim.try_borrow_mut()
                    && *sim.world.resource::<MakerMode>() == MakerMode::Play
                    && sim.world.resource::<Paused>().0
                {
                    sim.world.resource_mut::<PlayIntent>().reset_pressed = true;
                }
            }
            UiAction::MakerDismissClear => {
                self.menus.overlay = OverlayMenu::None;
                self.set_paused(false);
                self.set_mode(MakerMode::Edit);
            }
            UiAction::MakerRemix => {
                self.menus.overlay = OverlayMenu::None;
                self.set_paused(false);
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    sim.world
                        .resource_scope(|world, mut level: Mut<LevelDocument>| {
                            level.data.name = format!("Remix of {}", level.data.name);
                            maker::commands::invalidate_verification(&mut level);
                            *world.resource_mut::<maker::campaign::LevelSource>() =
                                maker::campaign::LevelSource::Editor;
                            world.resource_mut::<win::MakerUi>().current_key = None;
                            world.resource_mut::<SelectedEntity>().0 = None;
                            world.resource_mut::<ActiveTrack>().0 = None;
                        });
                }
                self.set_mode(MakerMode::Edit);
                self.announce("Remixing... level is yours now. Beat it to share!");
            }
            UiAction::PlayBundledLevel(i) => {
                let Some(data) = maker::campaign::load_bundled(i as usize) else {
                    return;
                };
                let name = data.name.clone();
                let outcome = match self.sim.try_borrow_mut() {
                    Ok(mut sim) => {
                        sim.world
                            .resource_scope(|world, mut level: Mut<LevelDocument>| {
                                let Ok(mut view) = self.world.try_borrow_mut() else {
                                    return Err(anyhow::anyhow!("view busy"));
                                };
                                view.release_stroke(&mut level);
                                storage::apply_level_data(&mut level, &mut view.history, data);
                                view.sync_source(&level);
                                *world.resource_mut::<maker::campaign::LevelSource>() =
                                    maker::campaign::LevelSource::Bundled(i as usize);
                                world.resource_mut::<win::MakerUi>().current_key = None;
                                world.resource_mut::<SelectedEntity>().0 = None;
                                world.resource_mut::<ActiveTrack>().0 = None;
                                Ok(())
                            })
                    }
                    Err(_) => return,
                };
                if outcome.is_err() {
                    return;
                }
                self.pending_mode = Some(MakerMode::Play);
                self.begin_loading();
                self.announce(format!("Playing: {name}"));
            }
            UiAction::MakerCloseSignDialog => {
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    let ui = &mut sim.world.resource_mut::<win::MakerUi>();
                    ui.sign_dialog_open = false;
                    ui.sign_dialog_lines.clear();
                }
            }
            UiAction::SetKeyboardCaptured(v) => {
                self.menus.keyboard_captured = v;
            }
            UiAction::BrowseOpen => {
                if let Ok(sim) = self.sim.try_borrow() {
                    let store = sim.world.resource::<storage::LevelStorage>();
                    self.menus.browse_levels = maker::catalog::build_catalog(store);
                }
                self.menus.browse_confirm_delete = None;
                self.menus.reconcile_browse_nav();
                self.menus.overlay = OverlayMenu::Browse;
            }
            UiAction::BrowsePlay(key) => {
                self.menus.browse_confirm_delete = None;
                self.menus.overlay = OverlayMenu::None;
                self.menu_load_level(&key, true);
                self.begin_loading();
            }
            UiAction::BrowseEdit(key) => {
                self.menus.browse_confirm_delete = None;
                self.menus.overlay = OverlayMenu::None;
                self.menu_load_level(&key, false);
                self.begin_loading();
            }
            UiAction::BrowseDelete(key) => {
                if self.menus.browse_confirm_delete.as_deref() == Some(key.as_str()) {
                    self.confirm_delete(&key);
                } else {
                    self.menus.browse_confirm_delete = Some(key);
                }
            }
            UiAction::BrowseConfirmDelete(key) => self.confirm_delete(&key),
            UiAction::BrowseCancelDelete => {
                self.menus.browse_confirm_delete = None;
            }
            UiAction::BrowseSelect(key) => {
                self.menus.browse_selected = Some(key);
                self.menus.browse_confirm_delete = None;
                self.menus.reconcile_browse_nav();
            }
            UiAction::BrowseClearSelection => {
                self.menus.browse_selected = None;
                self.menus.browse_confirm_delete = None;
            }
            UiAction::BrowseToggleTag(tag) => {
                if let Some(pos) = self
                    .menus
                    .browse_include_tags
                    .iter()
                    .position(|t| *t == tag)
                {
                    self.menus.browse_include_tags.remove(pos);
                } else {
                    self.menus.browse_include_tags.push(tag);
                }
                self.menus.browse_confirm_delete = None;
                self.menus.reconcile_browse_nav();
            }
            UiAction::BrowseToggleVerified => {
                self.menus.browse_verified_only = !self.menus.browse_verified_only;
                self.menus.browse_confirm_delete = None;
                self.menus.reconcile_browse_nav();
            }
            UiAction::BrowseSetDifficulty(d) => {
                self.menus.browse_difficulty = d;
                self.menus.browse_confirm_delete = None;
                self.menus.reconcile_browse_nav();
            }
            UiAction::BrowseCycleSort => {
                self.menus.browse_sort = (self.menus.browse_sort + 1) % 6;
                self.menus.reconcile_browse_nav();
            }
            UiAction::BrowseSetQuery(q) => {
                self.menus.browse_query = q;
                self.menus.browse_confirm_delete = None;
                self.menus.reconcile_browse_nav();
            }
            UiAction::BrowseClearQuery => {
                self.menus.browse_query.clear();
                self.menus.browse_confirm_delete = None;
                self.menus.reconcile_browse_nav();
            }
            UiAction::BrowsePublish(key) => {
                let loaded = {
                    let Ok(sim) = self.sim.try_borrow() else {
                        return;
                    };
                    let store = sim.world.resource::<storage::LevelStorage>();
                    store.0.load(&key)
                };
                let data = match loaded {
                    Ok(Some(text)) => storage::deserialize_level(&text),
                    Ok(None) => {
                        self.announce("Level not found.");
                        return;
                    }
                    Err(e) => Err(anyhow::anyhow!("Load failed: {e}")),
                };
                let data = match data {
                    Ok(d) => d,
                    Err(e) => {
                        self.announce(format!("{e}"));
                        return;
                    }
                };
                if !data.is_verified {
                    self.announce("Beat the level before publishing.");
                    return;
                }
                let meta = rustbox_format::api::UploadMetadata {
                    name: data.name.clone(),
                    description: data.description.clone(),
                    tags: data.tags.iter().map(|t| t.label().to_string()).collect(),
                    format_version: rustbox_format::file::FORMAT_VERSION,
                    game_version: env!("CARGO_PKG_VERSION").to_string(),
                };
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::Upload { meta, data });
                self.announce("Uploading...");
            }
            UiAction::OnlineOpen => {
                self.menus.browse_confirm_delete = None;
                self.menus.online_confirm_delete = None;
                let query = self.menus.online_query.clone();
                self.menus.online_loading = true;
                self.menus.online_last_offset = 0;
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::List {
                        query,
                        limit: 50,
                        offset: 0,
                    });
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::Me);
                self.menus.overlay = OverlayMenu::Online;
                self.announce("Loading online levels...");
            }
            UiAction::OnlineRefresh => {
                let query = self.menus.online_query.clone();
                self.menus.online_loading = true;
                self.menus.online_last_offset = 0;
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::List {
                        query,
                        limit: 50,
                        offset: 0,
                    });
                self.announce("Loading online levels...");
            }
            UiAction::OnlineLoadMore => {
                let offset = self.menus.online_levels.len() as u64;
                if offset < self.menus.online_total && !self.menus.online_loading {
                    let query = self.menus.online_query.clone();
                    self.menus.online_loading = true;
                    self.menus.online_last_offset = offset;
                    self.menus
                        .online_pending
                        .push(maker::online::OnlineRequest::List {
                            query,
                            limit: 50,
                            offset,
                        });
                    self.announce("Loading more levels...");
                }
            }
            UiAction::OnlineSelect(id) => {
                self.menus.online_selected = Some(id);
                self.menus.online_confirm_delete = None;
                self.menus.reconcile_online_nav();
            }
            UiAction::OnlineClearSelection => {
                self.menus.online_selected = None;
                self.menus.online_confirm_delete = None;
            }
            UiAction::OnlinePreview(id) => {
                // Cap concurrent preview downloads so a 50-level grid doesn't
                // stampede the network just to draw cards (LRU evicts later).
                const MAX_PREVIEW_IN_FLIGHT: usize = 4;

                if self.menus.online_previews.contains_key(&id)
                    || self.menus.online_preview_pending.contains(&id)
                    || self.menus.online_preview_pending.len() >= MAX_PREVIEW_IN_FLIGHT
                {
                    return;
                }
                let Some(meta) = self
                    .menus
                    .online_levels
                    .iter()
                    .find(|x| x.id == id)
                    .cloned()
                else {
                    return;
                };
                self.menus.online_preview_pending.push(meta.id);
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::Download { meta, play: false });
            }
            UiAction::OnlineSetShelf(shelf) => {
                self.menus.online_shelf = shelf;
                self.menus.online_confirm_delete = None;
                self.menus.sort_online_levels();
                self.menus.reconcile_online_nav();
                if shelf == 3 {
                    self.menus.online_loading = true;
                    self.menus
                        .online_pending
                        .push(maker::online::OnlineRequest::MyLevels);
                    self.announce("Loading your levels...");
                }
            }
            UiAction::OnlineSetIdQuery(q) => {
                self.menus.online_id_query = q;
            }
            UiAction::OnlineSearchId => {
                let id: u64 = self.menus.online_id_query.trim().parse().unwrap_or(0);
                if id == 0 {
                    self.announce("Enter a numeric level ID.");
                } else {
                    self.menus.online_loading = true;
                    self.menus
                        .online_pending
                        .push(maker::online::OnlineRequest::FetchById(id));
                    self.announce(format!("Searching #{id}"));
                }
            }
            UiAction::OnlinePlay(id) => {
                let meta = self
                    .menus
                    .online_levels
                    .iter()
                    .find(|x| x.id == id)
                    .cloned()
                    .unwrap_or_else(|| LevelMeta {
                        id,
                        author: String::new(),
                        name: format!("#{id}"),
                        description: String::new(),
                        tags: Vec::new(),
                        format_version: 0,
                        game_version: String::new(),
                        size_bytes: 0,
                        sha256: String::new(),
                        likes: 0,
                        plays: 0,
                        created_at: String::new(),
                        updated_at: String::new(),
                    });
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::Download { meta, play: true });
                self.menus.overlay = OverlayMenu::None;
                self.pending_mode = Some(MakerMode::Play);
                self.begin_loading();
                self.announce("Downloading level...");
            }
            UiAction::OnlineLike(id) => {
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::Like { id });
            }
            UiAction::OnlineReport(id) => {
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::Report { id });
            }
            UiAction::OnlineDelete(id) => {
                if self.menus.online_confirm_delete == Some(id) {
                    self.menus.online_confirm_delete = None;
                    self.menus
                        .online_pending
                        .push(maker::online::OnlineRequest::Delete { id });
                    self.announce(format!("Deleting #{id}..."));
                } else {
                    self.menus.online_confirm_delete = Some(id);
                }
            }
            UiAction::OnlineDeleteCancel => {
                self.menus.online_confirm_delete = None;
            }
            UiAction::OnlineClearQuery => {
                self.menus.online_query.clear();
                self.menus.online_loading = true;
                self.menus.online_last_offset = 0;
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::List {
                        query: String::new(),
                        limit: 50,
                        offset: 0,
                    });
                self.announce("Loading online levels...");
            }
            UiAction::OnlineSearch => {
                let query = self.menus.online_query.clone();
                self.menus.online_loading = true;
                self.menus.online_last_offset = 0;
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::List {
                        query,
                        limit: 50,
                        offset: 0,
                    });
                self.announce("Searching online levels...");
            }
            UiAction::OnlineSetQuery(q) => {
                self.menus.online_query = q;
            }
            UiAction::OnlineCycleSort => {
                self.menus.online_sort = (self.menus.online_sort + 1) % 4;
                self.menus.sort_online_levels();
                self.menus.reconcile_online_nav();
            }
            UiAction::OnlineSetToken(token) => {
                let token = token.trim().to_string();
                self.menus.online_token = token.clone();
                let msg = if token.is_empty() {
                    "Upload token cleared."
                } else {
                    "Upload token set."
                };
                self.announce(msg);
            }
            UiAction::CreatorImport(code) => {
                self.menus.creator_import_code = code;
            }
            UiAction::CreatorCopyKey => {
                let key = self.menus.creator_recovery_key.clone();
                if key.is_empty() {
                    self.announce("No recovery key yet, open the online browser once.");
                } else {
                    repose_core::clipboard::copy_to_clipboard(&key);
                    self.announce("Recovery key copied! Keep it private!! It is your account!!!");
                }
            }
            UiAction::OnlineUpload => {
                let checked = {
                    let Ok(sim) = self.sim.try_borrow() else {
                        return;
                    };
                    let level = sim.world.resource::<LevelDocument>();
                    if level.data.is_verified {
                        Some(level.data.clone())
                    } else {
                        None
                    }
                };
                let Some(data) = checked else {
                    self.announce("Beat the level before publishing.");
                    return;
                };
                let meta = rustbox_format::api::UploadMetadata {
                    name: data.name.clone(),
                    description: data.description.clone(),
                    tags: data.tags.iter().map(|t| t.label().to_string()).collect(),
                    format_version: rustbox_format::file::FORMAT_VERSION,
                    game_version: env!("CARGO_PKG_VERSION").to_string(),
                };
                self.menus
                    .online_pending
                    .push(maker::online::OnlineRequest::Upload { meta, data });
                self.announce("Uploading...");
            }
            UiAction::OpenPartPicker => {
                self.menus.overlay = OverlayMenu::PartPicker;
            }
            UiAction::MakerSetBrushTab(tab) => {
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    *sim.world.resource_mut::<BrushTab>() = match tab {
                        1 => BrushTab::Entities,
                        2 => BrushTab::Tracks,
                        _ => BrushTab::Blocks,
                    };
                }
            }
            UiAction::MakerSelectBlock(idx) => {
                if let Ok(mut world) = self.world.try_borrow_mut() {
                    world.brush.kind = maker::palette::block_from_index(idx);
                }
            }
            UiAction::MakerSelectEntity(idx) => {
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    sim.world.resource_mut::<SelectedEntityKind>().0 =
                        maker::palette::entity_from_index(idx);
                }
            }
            UiAction::MakerCycleShape => self.announce(self.brush_cycle_shape()),
            UiAction::MakerRotateBrushBlock => self.announce(self.brush_rotate_block()),
            UiAction::MakerToggleWaterlog => self.announce(self.brush_toggle_waterlog()),
            UiAction::MakerCycleLinkChannel => self.announce(self.brush_cycle_link()),
            UiAction::MakerRotateBrush => {
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    let mut yaw = sim.world.resource_mut::<PlaceYaw>();
                    yaw.0 = (yaw.0 + 45.0) % 360.0;
                }
            }
            UiAction::MakerUndo => self.undo_once(),
            UiAction::MakerRedo => self.redo_once(),
            UiAction::MakerSave => {
                let saved = {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    sim.world
                        .resource_scope(|world, mut level: Mut<LevelDocument>| {
                            let key = world
                                .resource::<win::MakerUi>()
                                .current_key
                                .clone()
                                .unwrap_or_else(|| storage::AUTOSAVE_KEY.to_string());
                            let store = world.resource::<storage::LevelStorage>();
                            let res = storage::save_level(store, &mut level, &key);
                            let mut ui = world.resource_mut::<win::MakerUi>();
                            match &res {
                                Ok(()) => {
                                    if ui.current_key.is_none() {
                                        ui.current_key = Some(key);
                                    }
                                    ui.set_status("Saved");
                                }
                                Err(e) => ui.set_status(format!("Save failed: {e}")),
                            }
                            res.is_ok()
                        })
                };
                if saved {
                    self.refresh_level_slots();
                    self.rebuild_catalog();
                }
            }
            UiAction::MakerOpenLoadPanel => {
                self.refresh_level_slots();
                self.menus.overlay = OverlayMenu::LoadLevel;
            }
            UiAction::MakerLoadSlot(name) => {
                let outcome = match self.sim.try_borrow_mut() {
                    Ok(mut sim) => {
                        sim.world
                            .resource_scope(|world, mut level: Mut<LevelDocument>| {
                                let store = world.resource::<storage::LevelStorage>();
                                let Ok(mut view) = self.world.try_borrow_mut() else {
                                    return Err(anyhow::anyhow!("view busy"));
                                };
                                view.release_stroke(&mut level);
                                let res = storage::load_level(
                                    store,
                                    &mut level,
                                    &mut view.history,
                                    &name,
                                );
                                if matches!(res, Ok(true)) {
                                    view.sync_source(&level);
                                }
                                res
                            })
                    }
                    Err(_) => return,
                };
                match outcome {
                    Ok(true) => {
                        if let Ok(mut sim) = self.sim.try_borrow_mut() {
                            sim.world.resource_mut::<SelectedEntity>().0 = None;
                            sim.world.resource_mut::<ActiveTrack>().0 = None;
                            let ui = &mut sim.world.resource_mut::<win::MakerUi>();
                            ui.current_key = Some(name.clone());
                            ui.set_status(format!("Loaded '{name}'"));
                        }
                        self.menus.overlay = OverlayMenu::None;
                    }
                    Ok(false) => self.announce("Slot empty"),
                    Err(e) => self.announce(format!("Load failed: {e}")),
                }
            }
            UiAction::MakerToggleMode => {
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    let mut mode = sim.world.resource_mut::<MakerMode>();
                    *mode = match *mode {
                        MakerMode::Edit => MakerMode::Play,
                        MakerMode::Play => MakerMode::Edit,
                    };
                }
            }
            UiAction::MakerInspParamDelta(delta) => {
                if let Some((id, old)) = self
                    .menus
                    .selected_entity_data
                    .as_ref()
                    .map(|e| (e.id, e.param))
                {
                    let new = (old + delta).clamp(0.0, 100.0);
                    if (new - old).abs() > 1e-4 {
                        self.apply_edit(EditCommand::SetEntityParam { id, old, new });
                    }
                }
            }
            UiAction::MakerInspYawDelta(delta) => {
                if let Some((id, old)) = self
                    .menus
                    .selected_entity_data
                    .as_ref()
                    .map(|e| (e.id, e.yaw_deg))
                {
                    let new = (old + delta).rem_euclid(360.0);
                    if (new - old).abs() > 1e-3 {
                        self.apply_edit(EditCommand::SetEntityYaw { id, old, new });
                    }
                }
            }
            UiAction::MakerInspLinkDelta(delta) => {
                if let Some((id, old)) = self
                    .menus
                    .selected_entity_data
                    .as_ref()
                    .map(|e| (e.id, e.link))
                {
                    let new = (old as i32 + delta).clamp(1, 9) as u32;
                    if new != old {
                        self.apply_edit(EditCommand::SetEntityLink { id, old, new });
                    }
                }
            }
            UiAction::MakerInspCycleContents => {
                let Some((id, kind, old, old_link)) = self
                    .menus
                    .selected_entity_data
                    .as_ref()
                    .map(|e| (e.id, e.kind, e.contents, e.link))
                else {
                    return;
                };
                if !kind.supports_contents() {
                    return;
                }
                let new = match old {
                    ContainedItem::None => ContainedItem::Glimmers(3),
                    ContainedItem::Glimmers(_) => ContainedItem::Key,
                    ContainedItem::Key => ContainedItem::HealOrb,
                    ContainedItem::HealOrb => ContainedItem::SpeedRing,
                    ContainedItem::SpeedRing => ContainedItem::None,
                };
                if matches!(new, ContainedItem::Key) && old_link == 0 {
                    let mut cmds = vec![EditCommand::SetEntityLink {
                        id,
                        old: old_link,
                        new: 1,
                    }];
                    if new != old {
                        cmds.push(EditCommand::SetEntityContents { id, old, new });
                    }
                    self.with_level(|history, level| history.apply_many(level, cmds));
                } else if new != old {
                    self.apply_edit(EditCommand::SetEntityContents { id, old, new });
                }
            }
            UiAction::MakerInspContentsDelta(delta) => {
                let Some((id, old)) = self
                    .menus
                    .selected_entity_data
                    .as_ref()
                    .map(|e| (e.id, e.contents))
                else {
                    return;
                };
                let ContainedItem::Glimmers(n) = old else {
                    return;
                };
                let new = ContainedItem::Glimmers((n as i32 + delta).clamp(1, 20) as u8);
                if new != old {
                    self.apply_edit(EditCommand::SetEntityContents { id, old, new });
                }
            }
            UiAction::MakerInspEditSignText => {
                if let Some((id, text)) = self
                    .menus
                    .selected_entity_data
                    .as_ref()
                    .map(|e| (e.id, e.sign_text.clone()))
                {
                    self.menus.sign_editor_open = true;
                    self.menus.sign_editor_id = id;
                    self.menus.sign_editor_text = text;
                }
            }
            UiAction::MakerInspSetSignText(text) => {
                if !self.menus.sign_editor_open {
                    return;
                }
                let id = self.menus.sign_editor_id;
                let changed = self
                    .with_level(|history, level| {
                        let Some(e) = level.entity_by_id(id) else {
                            return false;
                        };
                        let old = e.sign_text.clone();
                        let new: String =
                            text.chars().take(rustbox_format::MAX_SIGN_TEXT).collect();
                        if new != old {
                            history.apply(level, EditCommand::SetEntitySignText { id, old, new });
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if changed {
                    self.announce("Sign text updated");
                }
                self.menus.sign_editor_open = false;
                self.menus.sign_editor_text.clear();
            }
            UiAction::MakerInspCancelSignText => {
                self.menus.sign_editor_open = false;
                self.menus.sign_editor_text.clear();
            }
            UiAction::MakerInspTrackCycle => {
                let Some((id, old)) = self
                    .menus
                    .selected_entity_data
                    .as_ref()
                    .map(|e| (e.id, e.track))
                else {
                    return;
                };
                self.with_level(|history, level| {
                    let new = match old {
                        Some(_) => None,
                        None => {
                            let mut ids =
                                level.data.tracks.iter().map(|t| t.id).collect::<Vec<_>>();
                            ids.sort();
                            ids.first().copied()
                        }
                    };
                    if new != old {
                        history.apply(level, EditCommand::SetEntityTrack { id, old, new });
                    }
                });
            }
            UiAction::MakerInspDeleteEntity => {
                let Some(id) = self.menus.selected_entity_data.as_ref().map(|e| e.id) else {
                    return;
                };
                self.with_level(|history, level| {
                    if let Some(entity) = level.entity_by_id(id).cloned() {
                        history.apply(level, EditCommand::RemoveEntity { entity });
                    }
                });
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    sim.world.resource_mut::<SelectedEntity>().0 = None;
                }
            }
            UiAction::MakerInspTrackModeToggle => {
                let Some(id) = self.menus.active_track_data.as_ref().map(|t| t.id) else {
                    return;
                };
                self.with_level(|history, level| {
                    let Some(t) = level.track(id) else {
                        return;
                    };
                    let old = t.mode;
                    let new = match old {
                        TrackMode::PingPong => TrackMode::Loop,
                        TrackMode::Loop => TrackMode::PingPong,
                    };
                    history.apply(
                        level,
                        EditCommand::SetTrackMode {
                            track_id: id,
                            old,
                            new,
                        },
                    );
                });
            }
            UiAction::MakerInspTrackSpeedDelta(delta) => {
                let Some((id, old)) = self
                    .menus
                    .active_track_data
                    .as_ref()
                    .map(|t| (t.id, t.speed))
                else {
                    return;
                };
                let new = (old + delta).clamp(0.5, 10.0);
                if (new - old).abs() > 1e-4 {
                    self.apply_edit(EditCommand::SetTrackSpeed {
                        track_id: id,
                        old,
                        new,
                    });
                }
            }
            UiAction::MakerInspTrackReverse => {
                let Some(id) = self.menus.active_track_data.as_ref().map(|t| t.id) else {
                    return;
                };
                self.with_level(|history, level| {
                    if level.track(id).is_some() {
                        history.apply(level, EditCommand::ReverseTrackPoints { track_id: id });
                    }
                });
            }
            UiAction::MakerInspTrackDelete => {
                let Some(id) = self.menus.active_track_data.as_ref().map(|t| t.id) else {
                    return;
                };
                self.with_level(|history, level| {
                    if let Some(track) = level.track(id).cloned() {
                        let detached = maker::commands::detached_for(level, id);
                        history.apply(level, EditCommand::DeleteTrack { track, detached });
                    }
                });
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    sim.world.resource_mut::<ActiveTrack>().0 = None;
                }
            }
        }
    }

    fn apply_edit(&self, cmd: EditCommand) {
        self.with_level(|history, level| history.apply(level, cmd));
    }

    fn with_level<R>(
        &self,
        f: impl FnOnce(&mut CommandHistory, &mut LevelDocument) -> R,
    ) -> Option<R> {
        let mut sim = self.sim.try_borrow_mut().ok()?;
        let mut world = self.world.try_borrow_mut().ok()?;
        let mut level = sim.world.resource_mut::<LevelDocument>();
        Some(f(&mut world.history, &mut level))
    }

    fn refresh_level_slots(&mut self) {
        if let Ok(sim) = self.sim.try_borrow() {
            let store = sim.world.resource::<storage::LevelStorage>();
            self.menus.level_slots = storage::list_slots(store);
        }
    }

    fn undo_once(&mut self) {
        {
            let Ok(mut sim) = self.sim.try_borrow_mut() else {
                return;
            };
            let Ok(mut world) = self.world.try_borrow_mut() else {
                return;
            };
            let mut level = sim.world.resource_mut::<LevelDocument>();
            world.undo(&mut level);
        }
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            edit_ops::validate_refs(&mut sim.world);
        }
    }

    fn redo_once(&mut self) {
        {
            let Ok(mut sim) = self.sim.try_borrow_mut() else {
                return;
            };
            let Ok(mut world) = self.world.try_borrow_mut() else {
                return;
            };
            let mut level = sim.world.resource_mut::<LevelDocument>();
            world.redo(&mut level);
        }
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            edit_ops::validate_refs(&mut sim.world);
        }
    }

    fn brush_cycle_shape(&self) -> String {
        let Ok(mut world) = self.world.try_borrow_mut() else {
            return String::new();
        };
        let idx = ALL_BLOCK_SHAPES
            .iter()
            .position(|s| *s == world.brush.shape)
            .unwrap_or(0);
        world.brush.shape = ALL_BLOCK_SHAPES[(idx + 1) % ALL_BLOCK_SHAPES.len()];
        format!("Block shape: {}", world.brush.shape.name())
    }

    fn brush_rotate_block(&self) -> String {
        let Ok(mut world) = self.world.try_borrow_mut() else {
            return String::new();
        };
        world.brush.rot = (world.brush.rot.wrapping_add(1)) % 4;
        format!("Block rotation: {}°", (world.brush.rot as u16) * 90)
    }

    fn brush_toggle_waterlog(&self) -> String {
        let Ok(mut world) = self.world.try_borrow_mut() else {
            return String::new();
        };
        world.brush.waterlogged = !world.brush.waterlogged;
        if world.brush.waterlogged {
            "Waterlogged: on (blocks fill their cell with water)".to_string()
        } else {
            "Waterlogged: off".to_string()
        }
    }

    fn brush_cycle_link(&self) -> String {
        let Ok(mut sim) = self.sim.try_borrow_mut() else {
            return String::new();
        };
        let mut ch = sim.world.resource_mut::<ActiveLinkChannel>();
        ch.0 = ch.0 % 9 + 1;
        format!("Link channel: {}", ch.0)
    }

    fn confirm_delete(&mut self, key: &str) {
        let result = self.delete_slot(key);
        self.menus.browse_confirm_delete = None;
        match result {
            Ok(()) => {
                self.rebuild_catalog();
                self.announce("Deleted.");
            }
            Err(e) => self.announce(format!("Delete failed: {e}")),
        }
    }

    fn delete_slot(&self, key: &str) -> anyhow::Result<()> {
        let sim = self
            .sim
            .try_borrow()
            .map_err(|_| anyhow::anyhow!("sim busy"))?;
        let store = sim.world.resource::<storage::LevelStorage>();
        if key.starts_with(storage::COLLECTION_PREFIX) {
            storage::delete_collection(store, key)
        } else {
            store.0.delete(key)
        }
    }

    fn rebuild_catalog(&mut self) {
        let Ok(sim) = self.sim.try_borrow() else {
            return;
        };
        let store = sim.world.resource::<storage::LevelStorage>();
        self.menus.browse_levels = maker::catalog::build_catalog(store);
        self.menus.reconcile_browse_nav();
    }

    fn menu_load_level(&mut self, key: &str, play: bool) {
        let mut name = String::new();
        let outcome = match self.sim.try_borrow_mut() {
            Ok(mut sim) => sim
                .world
                .resource_scope(|world, mut level: Mut<LevelDocument>| {
                    let store = world.resource::<storage::LevelStorage>();
                    let Ok(mut view) = self.world.try_borrow_mut() else {
                        return Err(anyhow::anyhow!("view busy"));
                    };
                    view.release_stroke(&mut level);
                    let res = storage::load_level(store, &mut level, &mut view.history, key);
                    if matches!(res, Ok(true)) {
                        view.sync_source(&level);
                        name = level.data.name.clone();
                    }
                    res
                }),
            Err(_) => return,
        };
        match outcome {
            Ok(true) => {
                {
                    let Ok(mut sim) = self.sim.try_borrow_mut() else {
                        return;
                    };
                    sim.world.resource_mut::<SelectedEntity>().0 = None;
                    sim.world.resource_mut::<ActiveTrack>().0 = None;
                    *sim.world.resource_mut::<maker::campaign::LevelSource>() = if play {
                        maker::campaign::LevelSource::Imported
                    } else {
                        maker::campaign::LevelSource::Editor
                    };
                    let ui = &mut sim.world.resource_mut::<win::MakerUi>();
                    ui.current_key = Some(key.to_string());
                    ui.goal_latched = false;
                    ui.clear_deaths = 0;
                    ui.first_clear = false;
                    ui.new_record = false;
                    ui.player_is_author = false;
                    ui.clear_pending = false;
                    if play {
                        ui.play_timer = 0.0;
                        ui.deaths = 0;
                        ui.clear_time_secs = 0.0;
                    }
                }
                if play {
                    self.pending_mode = Some(MakerMode::Play);
                    self.announce(format!("Playing: {name}"));
                } else {
                    self.pending_mode = Some(MakerMode::Edit);
                    self.announce(format!("Editing: {name}"));
                }
            }
            Ok(false) => self.announce("Level not found."),
            Err(e) => self.announce(format!("Load failed: {e}")),
        }
    }

    fn begin_loading(&mut self) {
        self.menus.phase = menus::AppState::Loading;
        self.menus.overlay = menus::OverlayMenu::None;
        self.menus.loading_progress = 0.0;
        self.phase_t = 0.0;
    }

    fn tick_phase(&mut self, dt: f32) {
        self.phase_t += dt;
        match self.menus.phase {
            menus::AppState::Splash if self.phase_t >= 1.5 => {
                self.menus.phase = menus::AppState::Title;
                self.phase_t = 0.0;
            }
            menus::AppState::Loading => {
                self.menus.loading_progress = (self.phase_t / 0.5).min(1.0);
                if self.phase_t >= 0.5 {
                    self.enter_ingame();
                }
            }
            _ => {}
        }
    }

    fn enter_ingame(&mut self) {
        let mode = self.pending_mode.take().unwrap_or(MakerMode::Edit);
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            {
                let mut level = sim.world.resource_mut::<LevelDocument>();
                level.mark_all_dirty();
                level.entities_dirty = true;
            }
            *sim.world.resource_mut::<MakerMode>() = mode;
        }
        self.menus.phase = menus::AppState::InGame;
        self.phase_t = 0.0;
    }

    fn handle_pause_escape(&mut self, edges: &[PhysicalKey]) {
        if self.menus.phase != menus::AppState::InGame || !edges.contains(&PhysicalKey::Escape) {
            return;
        }
        if self.menus.sign_editor_open {
            self.menus.sign_editor_open = false;
            self.menus.sign_editor_text.clear();
            return;
        }
        let Ok(mut sim) = self.sim.try_borrow_mut() else {
            return;
        };
        let paused = sim.world.resource::<Paused>().0;
        match self.menus.overlay {
            menus::OverlayMenu::None if !paused => {
                self.menus.overlay = menus::OverlayMenu::Pause;
                sim.world.resource_mut::<Paused>().0 = true;
            }
            menus::OverlayMenu::Pause => {
                self.menus.overlay = menus::OverlayMenu::None;
                sim.world.resource_mut::<Paused>().0 = false;
            }
            menus::OverlayMenu::LoadLevel | menus::OverlayMenu::PartPicker => {
                self.menus.overlay = menus::OverlayMenu::None;
            }
            menus::OverlayMenu::Settings | menus::OverlayMenu::Credits => {
                self.menus.overlay = if paused {
                    menus::OverlayMenu::Pause
                } else {
                    menus::OverlayMenu::None
                };
            }
            menus::OverlayMenu::LevelInfo
            | menus::OverlayMenu::LevelClear
            | menus::OverlayMenu::Online => {
                self.menus.overlay = menus::OverlayMenu::None;
                sim.world.resource_mut::<Paused>().0 = false;
            }
            _ => {}
        }
    }

    fn update_input_capture(&mut self) {
        let modal = self.menus.overlay != menus::OverlayMenu::None
            || self.menus.phase != menus::AppState::InGame
            || self.menus.sign_dialog_open
            || self.menus.sign_editor_open
            || self.menus.keyboard_captured;
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            let mut capture = sim.world.resource_mut::<InputCapture>();
            capture.ui_wants_pointer = modal;
            capture.ui_wants_keyboard = modal;
        }
    }

    fn sync_menu_state(&mut self) {
        if self.menus.overlay != menus::OverlayMenu::Settings {
            let c = |v: f32, fb: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { fb };
            self.menus.master_vol = c(self.save.settings.master_volume, 1.0);
            self.menus.sfx_vol = c(self.save.settings.sfx_volume, 1.0);
            self.menus.music_vol = c(self.save.settings.music_volume, 0.8);
        }
        if self.menus.translations.is_empty() || self.menus.language != self.locale.current {
            self.menus.language = self.locale.current.clone();
            self.menus.translations = game_utils::i18n::get_current_translations(&self.locale);
        }
        self.menus.available_languages = self.locale.available.clone();
        let Ok(sim) = self.sim.try_borrow() else {
            return;
        };
        {
            let ui = sim.world.resource::<win::MakerUi>();
            self.menus.sign_dialog_open = ui.sign_dialog_open;
            self.menus.sign_dialog_lines = ui.sign_dialog_lines.clone();
            self.menus.maker_status = ui.status.clone();
            self.menus.play_time_secs = ui.play_timer;
            self.menus.deaths = ui.deaths;
            self.menus.glimmers_collected = ui.glimmers_collected;
            self.menus.glimmers_total = ui.glimmers_total;
            self.menus.clear_time_secs = ui.clear_time_secs;
            self.menus.clear_deaths = ui.clear_deaths;
            self.menus.first_clear = ui.first_clear;
            self.menus.new_record = ui.new_record;
            self.menus.player_is_author = ui.player_is_author;
        }
        self.menus.maker_mode_edit = *sim.world.resource::<MakerMode>() == MakerMode::Edit;
        self.menus.brush_tab = match *sim.world.resource::<BrushTab>() {
            BrushTab::Blocks => 0,
            BrushTab::Entities => 1,
            BrushTab::Tracks => 2,
        };
        self.menus.selected_entity =
            maker::palette::entity_index(sim.world.resource::<SelectedEntityKind>().0);
        self.menus.link_channel = sim.world.resource::<ActiveLinkChannel>().0;
        self.menus.mirror = sim.world.resource::<MirrorMode>().0;
        {
            let stats = sim.world.resource::<maker::limits::LevelStats>();
            self.menus.limit_blocks = stats.blocks;
            self.menus.limit_entities = stats.entities;
            self.menus.limit_tracks = stats.tracks;
            self.menus.limit_vertices = stats.estimated_vertices;
            self.menus.limit_warning = stats.warning;
            self.menus.limit_over = stats.over_limit;
            self.menus.blocks_placed = stats.blocks;
        }
        {
            let level = sim.world.resource::<LevelDocument>();
            self.menus.level_name = level.data.name.clone();
            self.menus.level_verified = level.data.is_verified;
            self.menus.record_ms = level.data.record_ms;
            let sel = sim.world.resource::<SelectedEntity>().0;
            self.menus.selected_entity_data = sel.and_then(|id| level.entity_by_id(id)).cloned();
            let track = sim.world.resource::<ActiveTrack>().0;
            self.menus.active_track_data = track.and_then(|id| level.track(id)).cloned();
        }
        self.menus.is_bundled = *sim.world.resource::<maker::campaign::LevelSource>()
            != maker::campaign::LevelSource::Editor;
        {
            let progress = sim.world.resource::<maker::campaign::CampaignProgress>();
            self.menus.campaign_levels = maker::campaign::BUNDLED_LEVELS
                .iter()
                .map(|b| {
                    let rec = progress.record(b.id);
                    maker::campaign::CampaignLevelUi {
                        title: b.name.to_string(),
                        teaches: b.teaches.to_string(),
                        completed: rec.completed,
                        best_time: rec.best_time,
                        best_deaths: rec.best_deaths,
                    }
                })
                .collect();
        }
        {
            let Ok(view) = self.world.try_borrow() else {
                return;
            };
            self.menus.selected_block = maker::palette::block_index(view.brush.kind);
            self.menus.brush_shape = view.brush.shape as u8;
            self.menus.brush_rot = view.brush.rot;
            self.menus.waterlogged = view.brush.waterlogged;
            self.menus.can_undo = !view.history.undo.is_empty();
            self.menus.can_redo = !view.history.redo.is_empty();
        }
        if let Some(p) = sim.world.get::<Player>(self.player) {
            self.menus.player_armor = p.armor;
            self.menus.player_keys = p.keys;
        } else {
            self.menus.player_armor = 0;
            self.menus.player_keys = [0; 10];
        }
    }

    fn set_paused(&self, paused: bool) {
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            sim.world.resource_mut::<Paused>().0 = paused;
        }
    }

    fn is_paused(&self) -> bool {
        self.sim
            .try_borrow()
            .is_ok_and(|sim| sim.world.resource::<Paused>().0)
    }

    fn set_mode(&self, mode: MakerMode) {
        if let Ok(mut sim) = self.sim.try_borrow_mut() {
            *sim.world.resource_mut::<MakerMode>() = mode;
        }
    }

    fn undo_redo(&mut self, sched: &Scheduler, edges: &[PhysicalKey]) {
        let ctrl = sched.held_keys.contains(&PhysicalKey::ControlLeft)
            || sched.held_keys.contains(&PhysicalKey::ControlRight);
        if !ctrl || (!edges.contains(&PhysicalKey::KeyZ) && !edges.contains(&PhysicalKey::KeyY)) {
            return;
        }
        if edges.contains(&PhysicalKey::KeyY) {
            self.redo_once();
        } else {
            self.undo_once();
        }
    }

    fn frame_selection(&self, sched: &Scheduler, edges: &[PhysicalKey]) {
        let shift = sched.held_keys.contains(&PhysicalKey::ShiftLeft)
            || sched.held_keys.contains(&PhysicalKey::ShiftRight);
        if !shift || !edges.contains(&PhysicalKey::KeyF) {
            return;
        }
        let sim = self.sim.borrow();
        let selection = sim.world.resource::<SelectionSet>();
        let level = sim.world.resource::<LevelDocument>();
        let mut min = IVec3::splat(i32::MAX);
        let mut max = IVec3::splat(i32::MIN);
        let mut has = false;
        for &cell in &selection.blocks {
            if level.get_block(cell).is_some() {
                min = min.min(cell);
                max = max.max(cell + IVec3::ONE);
                has = true;
            }
        }
        for &id in &selection.entities {
            if let Some(entity) = level.entity_by_id(id) {
                let cell = entity.cell_i();
                min = min.min(cell);
                max = max.max(cell + IVec3::ONE);
                has = true;
            }
        }
        let (min, max) = if has {
            (min, max)
        } else {
            let (bmin, bmax) = level.play_bounds();
            (bmin, bmax + IVec3::ONE)
        };
        let center = (min.as_vec3() + max.as_vec3()) * 0.5;
        let extents = max.as_vec3() - min.as_vec3();
        let mut cam = self.cam.get();
        cam.target = center;
        cam.dist = (extents.length() * 1.35).clamp(8.0, 80.0);
        self.cam.set(cam);
    }

    fn hotkeys(&mut self, sched: &mut Scheduler, edges: &[PhysicalKey]) {
        let (mode, kb_ok, ptr_ok, paste_on, tab) = {
            let sim = self.sim.borrow();
            let capture = sim.world.resource::<InputCapture>();
            (
                *sim.world.resource::<MakerMode>(),
                !capture.ui_wants_keyboard,
                !capture.ui_wants_pointer,
                sim.world.resource::<PastePreview>().active,
                *sim.world.resource::<BrushTab>(),
            )
        };
        if mode != MakerMode::Edit {
            return;
        }
        let ctrl = sched.held_keys.contains(&PhysicalKey::ControlLeft)
            || sched.held_keys.contains(&PhysicalKey::ControlRight);
        let shift = sched.held_keys.contains(&PhysicalKey::ShiftLeft)
            || sched.held_keys.contains(&PhysicalKey::ShiftRight);
        let edge = |key: PhysicalKey| edges.contains(&key);

        if kb_ok && ctrl && edge(PhysicalKey::KeyS) {
            let outcome = match self.sim.try_borrow_mut() {
                Ok(mut sim) => sim
                    .world
                    .resource_scope(|world, mut level: Mut<LevelDocument>| {
                        let store = world.resource::<storage::LevelStorage>();
                        storage::save_level(store, &mut level, storage::AUTOSAVE_KEY)
                    }),
                Err(_) => return,
            };
            match outcome {
                Ok(()) => self.announce("Level saved"),
                Err(e) => self.announce(format!("Save failed: {e}")),
            }
        }
        if kb_ok && ctrl && edge(PhysicalKey::KeyL) {
            let outcome = match self.sim.try_borrow_mut() {
                Ok(mut sim) => sim
                    .world
                    .resource_scope(|world, mut level: Mut<LevelDocument>| {
                        let store = world.resource::<storage::LevelStorage>();
                        let Ok(mut view) = self.world.try_borrow_mut() else {
                            return Err(anyhow::anyhow!("view busy"));
                        };
                        view.release_stroke(&mut level);
                        let res = storage::load_level(
                            store,
                            &mut level,
                            &mut view.history,
                            storage::AUTOSAVE_KEY,
                        );
                        if matches!(res, Ok(true)) {
                            view.sync_source(&level);
                        }
                        res
                    }),
                Err(_) => return,
            };
            match outcome {
                Ok(true) => self.announce("Level loaded"),
                Ok(false) => self.announce("No saved level found"),
                Err(e) => self.announce(format!("Load failed: {e}")),
            }
        }

        if paste_on {
            if edge(PhysicalKey::Escape) {
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    sim.world.resource_mut::<PastePreview>().reset();
                    sim.world.resource_mut::<SelectionBoxStart>().start = None;
                }
                self.announce("Paste preview cancelled");
            } else if kb_ok && ptr_ok && edge(PhysicalKey::KeyR) {
                let yaw = match self.sim.try_borrow_mut() {
                    Ok(mut sim) => {
                        let mut pv = sim.world.resource_mut::<PastePreview>();
                        edit_ops::rotate_yaw(&mut pv.yaw);
                        pv.yaw
                    }
                    Err(_) => return,
                };
                self.announce(format!("Preview rotated to {yaw}°"));
            }
            if kb_ok {
                self.undo_redo(sched, edges);
                self.frame_selection(sched, edges);
            }
            return;
        }

        if !kb_ok {
            return;
        }

        if edge(PhysicalKey::KeyQ) {
            if let Ok(mut sim) = self.sim.try_borrow_mut() {
                sim.world.resource_mut::<BoxFillStart>().start = None;
                let mut tab = sim.world.resource_mut::<BrushTab>();
                *tab = match *tab {
                    BrushTab::Blocks => BrushTab::Entities,
                    BrushTab::Entities => BrushTab::Tracks,
                    BrushTab::Tracks => BrushTab::Blocks,
                };
            }
            if let Ok(mut world) = self.world.try_borrow_mut()
                && let Ok(mut sim) = self.sim.try_borrow_mut()
            {
                let mut level = sim.world.resource_mut::<LevelDocument>();
                world.release_stroke(&mut level);
            }
        }

        if edge(PhysicalKey::KeyF)
            && !shift
            && let Ok(mut sim) = self.sim.try_borrow_mut()
        {
            let mut yaw = sim.world.resource_mut::<PlaceYaw>();
            yaw.0 = (yaw.0 + 45.0) % 360.0;
        }

        if edge(PhysicalKey::KeyL) {
            let msg = self.brush_cycle_link();
            self.announce(msg);
        }

        if tab == BrushTab::Blocks {
            let picked: Option<(BlockKind, &'static str)> = edge(PhysicalKey::Digit1)
                .then_some((BlockKind::Grass, ""))
                .or_else(|| edge(PhysicalKey::Digit2).then_some((BlockKind::Stone, "")))
                .or_else(|| edge(PhysicalKey::Digit3).then_some((BlockKind::Hazard, "")))
                .or_else(|| edge(PhysicalKey::Digit4).then_some((BlockKind::Goal, "")))
                .or_else(|| edge(PhysicalKey::Digit5).then_some((BlockKind::Spawn, "")))
                .or_else(|| {
                    edge(PhysicalKey::Digit6)
                        .then_some((BlockKind::Water, "Water: fills a cell with swimmable water"))
                })
                .or_else(|| {
                    edge(PhysicalKey::Digit7).then_some((BlockKind::Ice, "Ice: slippery surface"))
                })
                .or_else(|| {
                    edge(PhysicalKey::Digit8)
                        .then_some((BlockKind::Spikes, "Spikes: floor/ceiling hazard"))
                })
                .or_else(|| {
                    edge(PhysicalKey::Digit9).then_some((
                        BlockKind::Conveyor,
                        "Conveyor: pushes along its facing (R rotates)",
                    ))
                })
                .or_else(|| {
                    edge(PhysicalKey::Digit0)
                        .then_some((BlockKind::Bounce, "Bounce: springs you up"))
                })
                .or_else(|| {
                    edge(PhysicalKey::KeyY).then_some((
                        BlockKind::OneWay,
                        "One-Way: land on top, pass through from below/sides",
                    ))
                })
                .or_else(|| {
                    edge(PhysicalKey::KeyP).then_some((
                        BlockKind::TimedPulse,
                        "Timed Pulse: solid while on/off channel is ON",
                    ))
                });
            if let Some((kind, msg)) = picked {
                if let Ok(mut world) = self.world.try_borrow_mut() {
                    world.brush.kind = kind;
                }
                if !msg.is_empty() {
                    self.announce(msg);
                }
            }
            let mut status: Option<String> = None;
            if edge(PhysicalKey::KeyR) {
                status = Some(self.brush_rotate_block());
            }
            if edge(PhysicalKey::KeyT) {
                status = Some(self.brush_cycle_shape());
            }
            if edge(PhysicalKey::KeyU) {
                status = Some(self.brush_toggle_waterlog());
            }
            if let Some(status) = status {
                self.announce(status);
            }
        }

        if tab == BrushTab::Entities {
            let picked = edge(PhysicalKey::Digit1)
                .then_some(EntityKind::Glimmer)
                .or_else(|| edge(PhysicalKey::Digit2).then_some(EntityKind::LaunchPad))
                .or_else(|| edge(PhysicalKey::Digit3).then_some(EntityKind::Seal))
                .or_else(|| edge(PhysicalKey::Digit4).then_some(EntityKind::DriftPlate))
                .or_else(|| edge(PhysicalKey::Digit5).then_some(EntityKind::Prowler))
                .or_else(|| edge(PhysicalKey::Digit6).then_some(EntityKind::TriggerOrb))
                .or_else(|| edge(PhysicalKey::Digit7).then_some(EntityKind::RelayGate))
                .or_else(|| edge(PhysicalKey::Digit8).then_some(EntityKind::Teleporter))
                .or_else(|| edge(PhysicalKey::Digit9).then_some(EntityKind::Fan))
                .or_else(|| edge(PhysicalKey::Digit0).then_some(EntityKind::Bumper));
            if let Some(kind) = picked
                && let Ok(mut sim) = self.sim.try_borrow_mut()
            {
                sim.world.resource_mut::<SelectedEntityKind>().0 = kind;
            }
        }

        let active_track = if tab == BrushTab::Tracks {
            let sim = self.sim.borrow();
            sim.world.resource::<ActiveTrack>().0
        } else {
            None
        };
        if let Some(id) = active_track {
            if edge(PhysicalKey::Enter) || edge(PhysicalKey::Escape) {
                if let Ok(mut sim) = self.sim.try_borrow_mut() {
                    sim.world.resource_mut::<ActiveTrack>().0 = None;
                }
            } else if edge(PhysicalKey::KeyM)
                || edge(PhysicalKey::Equal)
                || edge(PhysicalKey::Minus)
                || edge(PhysicalKey::NumpadAdd)
                || edge(PhysicalKey::NumpadSubtract)
            {
                let (track_mode, speed) = {
                    let sim = self.sim.borrow();
                    let level = sim.world.resource::<LevelDocument>();
                    level
                        .track(id)
                        .map(|t| (t.mode, t.speed))
                        .unwrap_or((TrackMode::PingPong, DEFAULT_TRACK_SPEED))
                };
                let cmd = if edge(PhysicalKey::KeyM) {
                    EditCommand::SetTrackMode {
                        track_id: id,
                        old: track_mode,
                        new: match track_mode {
                            TrackMode::PingPong => TrackMode::Loop,
                            TrackMode::Loop => TrackMode::PingPong,
                        },
                    }
                } else if edge(PhysicalKey::Equal) || edge(PhysicalKey::NumpadAdd) {
                    EditCommand::SetTrackSpeed {
                        track_id: id,
                        old: speed,
                        new: (speed + 0.5).min(10.0),
                    }
                } else {
                    EditCommand::SetTrackSpeed {
                        track_id: id,
                        old: speed,
                        new: (speed - 0.5).max(0.5),
                    }
                };
                if let Ok(mut sim) = self.sim.try_borrow_mut()
                    && let Ok(mut world) = self.world.try_borrow_mut()
                {
                    let mut level = sim.world.resource_mut::<LevelDocument>();
                    world.history.apply(&mut level, cmd);
                }
            }
        }

        if edge(PhysicalKey::KeyV) && !ctrl {
            let label = match self.sim.try_borrow_mut() {
                Ok(mut sim) => {
                    let mut mirror = sim.world.resource_mut::<MirrorMode>();
                    mirror.0 = (mirror.0 + 1) % 4;
                    match mirror.0 {
                        0 => "Off",
                        1 => "X",
                        2 => "Z",
                        _ => "X+Z",
                    }
                    .to_string()
                }
                Err(_) => return,
            };
            self.announce(format!("Mirror: {label}"));
        }

        if edge(PhysicalKey::Escape) {
            if let Ok(mut sim) = self.sim.try_borrow_mut() {
                sim.world.resource_mut::<SelectionSet>().clear();
                sim.world.resource_mut::<SelectionBoxStart>().start = None;
                sim.world.resource_mut::<BoxFillStart>().start = None;
                sim.world.resource_mut::<SelectedEntity>().0 = None;
            }
            self.announce("Selection cleared");
        } else if edge(PhysicalKey::KeyB) {
            let anchor = {
                let sim = self.sim.borrow();
                let cursor = sim.world.resource::<EditorCursor>();
                cursor.hit.or(cursor.place)
            };
            if let Some(anchor) = anchor {
                let start = {
                    let sim = self.sim.borrow();
                    sim.world.resource::<SelectionBoxStart>().start
                };
                match start {
                    None => {
                        if let Ok(mut sim) = self.sim.try_borrow_mut() {
                            sim.world.resource_mut::<SelectionBoxStart>().start = Some(anchor);
                        }
                        self.announce("Selection corner set. Press B on the opposite corner.");
                    }
                    Some(a) => {
                        let min = a.min(anchor);
                        let max = a.max(anchor);
                        let (blocks, entities) = {
                            let sim = self.sim.borrow();
                            let level = sim.world.resource::<LevelDocument>();
                            let blocks: Vec<IVec3> = level
                                .map
                                .keys()
                                .copied()
                                .filter(|c| edit_ops::cell_in_aabb(*c, min, max))
                                .collect();
                            let entities: Vec<_> = level
                                .data
                                .entities
                                .iter()
                                .filter(|e| edit_ops::cell_in_aabb(e.cell_i(), min, max))
                                .map(|e| e.id)
                                .collect();
                            (blocks, entities)
                        };
                        let count = if let Ok(mut sim) = self.sim.try_borrow_mut() {
                            sim.world.resource_mut::<SelectionBoxStart>().start = None;
                            let count = {
                                let mut sel = sim.world.resource_mut::<SelectionSet>();
                                if !shift {
                                    sel.clear();
                                }
                                sel.blocks.extend(blocks);
                                sel.entities.extend(entities);
                                sel.len()
                            };
                            let first = sim
                                .world
                                .resource::<SelectionSet>()
                                .entities
                                .iter()
                                .next()
                                .copied();
                            sim.world.resource_mut::<SelectedEntity>().0 = first;
                            count
                        } else {
                            0
                        };
                        self.announce(format!("Volume selected {count} item(s)"));
                    }
                }
            } else {
                self.announce("Aim at the level to volume-select");
            }
        } else if ctrl && edge(PhysicalKey::KeyA) {
            if let Ok(mut sim) = self.sim.try_borrow_mut() {
                let (blocks, entities) = {
                    let level = sim.world.resource::<LevelDocument>();
                    (
                        level.map.keys().copied().collect::<Vec<_>>(),
                        level.data.entities.iter().map(|e| e.id).collect::<Vec<_>>(),
                    )
                };
                let count = {
                    let mut sel = sim.world.resource_mut::<SelectionSet>();
                    sel.clear();
                    sel.blocks.extend(blocks);
                    sel.entities.extend(entities);
                    sel.len()
                };
                let first = sim
                    .world
                    .resource::<SelectionSet>()
                    .entities
                    .iter()
                    .next()
                    .copied();
                sim.world.resource_mut::<SelectedEntity>().0 = first;
                drop(sim);
                self.announce(format!("Selected all {count} item(s)"));
            }
        } else if ctrl && edge(PhysicalKey::KeyC) {
            if let Ok(mut sim) = self.sim.try_borrow_mut() {
                let selection = sim.world.resource::<SelectionSet>().clone();
                let mut clipboard =
                    std::mem::take(&mut *sim.world.resource_mut::<EditorClipboard>());
                let count = {
                    let level = sim.world.resource::<LevelDocument>();
                    edit_ops::copy_selection_to_clipboard(&level, &selection, &mut clipboard)
                };
                *sim.world.resource_mut::<EditorClipboard>() = clipboard;
                drop(sim);
                self.announce(if count == 0 {
                    "Nothing selected to copy".to_string()
                } else {
                    format!("Copied {count} item(s)")
                });
            }
        } else if ctrl && edge(PhysicalKey::KeyX) {
            if let Ok(mut sim) = self.sim.try_borrow_mut() {
                let selection = sim.world.resource::<SelectionSet>().clone();
                let mut clipboard =
                    std::mem::take(&mut *sim.world.resource_mut::<EditorClipboard>());
                let copied = {
                    let level = sim.world.resource::<LevelDocument>();
                    edit_ops::copy_selection_to_clipboard(&level, &selection, &mut clipboard)
                };
                *sim.world.resource_mut::<EditorClipboard>() = clipboard;
                if copied == 0 {
                    drop(sim);
                    self.announce("Nothing selected to cut");
                } else {
                    let mut sel = selection;
                    let mut sel_ent = SelectedEntity(sim.world.resource::<SelectedEntity>().0);
                    let deleted = {
                        let mut level = sim.world.resource_mut::<LevelDocument>();
                        let Ok(mut world) = self.world.try_borrow_mut() else {
                            return;
                        };
                        world.delete_selection(&mut level, &mut sel, &mut sel_ent)
                    };
                    *sim.world.resource_mut::<SelectionSet>() = sel;
                    sim.world.resource_mut::<SelectedEntity>().0 = sel_ent.0;
                    drop(sim);
                    self.announce(format!("Cut {deleted} item(s)"));
                }
            }
        } else if ctrl && edge(PhysicalKey::KeyV) {
            if let Ok(mut sim) = self.sim.try_borrow_mut() {
                let empty = sim.world.resource::<EditorClipboard>().is_empty();
                let target = {
                    let cursor = sim.world.resource::<EditorCursor>();
                    cursor.place.or(cursor.hit)
                };
                if empty {
                    drop(sim);
                    self.announce("Clipboard is empty");
                } else if let Some(target) = target {
                    let clipboard = sim.world.resource::<EditorClipboard>().clone();
                    {
                        let mut pv = sim.world.resource_mut::<PastePreview>();
                        pv.active = true;
                        pv.clipboard = clipboard;
                        pv.current_pivot = target;
                        pv.yaw = 0.0;
                    }
                    drop(sim);
                    self.announce(
                        "Paste preview active - Left click to place, R to rotate, Right click/Escape to cancel",
                    );
                } else {
                    drop(sim);
                    self.announce("Aim at the level to start paste preview");
                }
            }
        } else if edge(PhysicalKey::Delete) {
            if let Ok(mut sim) = self.sim.try_borrow_mut() {
                let sel_ent_before = sim.world.resource::<SelectedEntity>().0;
                let mut deleted: Option<usize> = None;
                if !sim.world.resource::<SelectionSet>().is_empty() {
                    let mut sel = sim.world.resource::<SelectionSet>().clone();
                    let mut sel_ent = SelectedEntity(sel_ent_before);
                    let count = {
                        let mut level = sim.world.resource_mut::<LevelDocument>();
                        let Ok(mut world) = self.world.try_borrow_mut() else {
                            return;
                        };
                        world.delete_selection(&mut level, &mut sel, &mut sel_ent)
                    };
                    *sim.world.resource_mut::<SelectionSet>() = sel;
                    sim.world.resource_mut::<SelectedEntity>().0 = sel_ent.0;
                    deleted = Some(count);
                }
                if let Some(id) = sel_ent_before {
                    let entity = {
                        let level = sim.world.resource::<LevelDocument>();
                        level.entity_by_id(id).cloned()
                    };
                    if let Some(entity) = entity
                        && let Ok(mut world) = self.world.try_borrow_mut()
                    {
                        let mut level = sim.world.resource_mut::<LevelDocument>();
                        world
                            .history
                            .apply(&mut level, EditCommand::RemoveEntity { entity });
                        sim.world.resource_mut::<SelectedEntity>().0 = None;
                    }
                }
                drop(sim);
                if let Some(count) = deleted {
                    self.announce(format!("Deleted {count} item(s)"));
                }
            }
        }

        self.undo_redo(sched, edges);
        self.frame_selection(sched, edges);
    }
}

fn ground() -> repame_view3d::MeshGroup {
    let mut group = repame_view3d::MeshGroup {
        depth_test: true,
        ..repame_view3d::MeshGroup::default()
    };
    const HALF: i32 = 12;
    for z in -HALF..HALF {
        for x in -HALF..HALF {
            let x0 = x as f32;
            let z0 = z as f32;
            let tint = if (x + z).rem_euclid(2) == 0 {
                [0.45, 0.58, 0.4]
            } else {
                [0.33, 0.45, 0.3]
            };
            group.push_quad_lit(
                [x0, 0.0, z0],
                [x0, 0.0, z0 + 1.0],
                [x0 + 1.0, 0.0, z0 + 1.0],
                [x0 + 1.0, 0.0, z0],
                tint,
                [0.0, 1.0, 0.0],
            );
        }
    }
    group
}

fn player_box(player: &Player, tf: &PlayerTransform) -> repame_view3d::MeshGroup {
    let mut group = repame_view3d::MeshGroup {
        depth_test: true,
        ..repame_view3d::MeshGroup::default()
    };
    let he = player.half_extents * tf.scale;
    let (x0, y0, z0) = (
        tf.translation.x - he.x,
        tf.translation.y - he.y,
        tf.translation.z - he.z,
    );
    let (x1, y1, z1) = (
        tf.translation.x + he.x,
        tf.translation.y + he.y,
        tf.translation.z + he.z,
    );
    let tint = [0.62, 0.7, 0.85];
    group.push_quad_lit(
        [x0, y1, z0],
        [x0, y1, z1],
        [x1, y1, z1],
        [x1, y1, z0],
        tint,
        [0.0, 1.0, 0.0],
    );
    group.push_quad_lit(
        [x0, y0, z0],
        [x1, y0, z0],
        [x1, y0, z1],
        [x0, y0, z1],
        tint,
        [0.0, -1.0, 0.0],
    );
    group.push_quad_lit(
        [x1, y0, z0],
        [x1, y1, z0],
        [x1, y1, z1],
        [x1, y0, z1],
        tint,
        [1.0, 0.0, 0.0],
    );
    group.push_quad_lit(
        [x0, y0, z0],
        [x0, y0, z1],
        [x0, y1, z1],
        [x0, y1, z0],
        tint,
        [-1.0, 0.0, 0.0],
    );
    group.push_quad_lit(
        [x0, y0, z1],
        [x1, y0, z1],
        [x1, y1, z1],
        [x0, y1, z1],
        tint,
        [0.0, 0.0, 1.0],
    );
    group.push_quad_lit(
        [x0, y0, z0],
        [x0, y1, z0],
        [x1, y1, z0],
        [x1, y0, z0],
        tint,
        [0.0, 0.0, -1.0],
    );
    group
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn run() {
    let mut app = App::new();
    #[cfg(not(target_arch = "wasm32"))]
    repame_shell::run_desktop("Rustbox", (1280, 720), move |sched, ctx| {
        app.view(sched, ctx)
    })
    .expect("rustbox failed to start");
    #[cfg(target_arch = "wasm32")]
    {
        let _ = repame_shell::run_web(move |sched, ctx| app.view(sched, ctx));
    }
}
