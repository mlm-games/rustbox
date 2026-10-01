pub mod maker;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::IntoScheduleConfigs;
use glam::Vec3;
use maker::camera::{CameraRig, play_camera_follow};
use maker::entities_runtime::{
    DropIdCounter, EntityEntities, LinkState, apply_fans, carry_crate_riders,
    collect_dropped_glimmers, collect_glimmers, collect_heal_orbs, collect_keys,
    despawn_drops_when_dirty, draw_props, move_prowlers, rebuild_runtime_solids,
    reconcile_entities, tick_drift_plates, tick_track_followers, touch_checkpoints,
    touch_speed_rings, update_crumble_plates, update_drops, update_lock_gates, update_relay_gates,
    update_seals,
};
use maker::interaction::{DamageRequests, ForcedMotionRequests, InteractionMemory, UseSelection};
use maker::interactive_blocks::{
    OnOffState, PulseClock, reset_onoff_state, reset_pulse_clock, sync_pulse, touch_onoff_switches,
};
use maker::level::{LevelDocument, raycast_present};
use maker::level_file::deserialize_level;
use maker::level_view::LevelView;
use maker::mode::{EditorCursor, InputCapture, MakerMode, MirrorMode};
use maker::player::{
    MoveState, MoveTuning, PlayIntent, PlayKeys, Player, PlayerTransform, PressedLatch, Trauma,
    clear_pressed_latch, latch_play_presses, player_controller, spawn_player, sync_mode,
};
use maker::props::RuntimeSolids;
use maker::{Paused, interaction, not_paused, rapier, win};
use repame_shell::{Sim, SimTime, Staging};
use repame_view3d::{BatchDesc, Frame3d, GeomHandle, OrbitCamera, View3dEvent, Viewport3d};
use repose_core::input::{KeyEvent, PhysicalKey, PointerButton, PointerEvent, PointerEventKind};
use repose_core::{
    Color, CursorIcon, Dp, FocusRequester, Modifier, RenderContext, Scheduler, Sp, View, remember,
    request_frame,
};
use repose_ui::{Column, Text, TextStyle, ViewExt, ZStack};
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const BUNDLED_LEVEL: &str = include_str!("../assets/levels/01_first_steps.ron");

struct App {
    sim: Rc<RefCell<Sim>>,
    cam: Rc<Cell<OrbitCamera>>,
    world: Rc<RefCell<LevelView>>,
    button: Rc<Cell<Option<PointerButton>>>,
    ctrl: Rc<Cell<bool>>,
    staging: Rc<RefCell<Staging>>,
    player: Entity,
    undo_latched: bool,
    redo_latched: bool,
    prev_mode: MakerMode,
    looked: Rc<Cell<bool>>,
    last: Instant,
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
        sim.world.insert_resource(PulseClock::default());
        sim.world.insert_resource(InteractionMemory::default());
        sim.world.insert_resource(ForcedMotionRequests::default());
        sim.world.insert_resource(UseSelection::default());
        sim.world.insert_resource(DamageRequests::default());
        sim.world.insert_resource(LinkState::default());
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
            ctrl: Rc::new(Cell::new(false)),
            staging: Staging::shared(),
            player,
            undo_latched: false,
            redo_latched: false,
            prev_mode: MakerMode::Edit,
            looked: Rc::new(Cell::new(false)),
            last: Instant::now(),
        }
    }

    fn view(&mut self, sched: &mut Scheduler, _ctx: &RenderContext) -> View {
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
            self.prev_mode = mode;
        }
        win::retry_play(&mut sim.world, mode);
        let entities_rebuilt = sim.world.resource::<LevelDocument>().entities_dirty;
        sim.step(dt);
        rapier::write_back_bodies(&mut sim.world);
        rapier::move_held_objects(&mut sim.world);
        rapier::pickup_throwables(&mut sim.world);
        let elapsed = sim.world.resource::<SimTime>().elapsed_secs;
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
        if mode == MakerMode::Play {
            self.cam.set(cam);
        }
        let paused = sim.world.resource::<Paused>().0;
        sched.cursor_override = if mode == MakerMode::Play && !paused {
            Some(CursorIcon::Hidden)
        } else {
            None
        };
        let in_edit = mode == MakerMode::Edit;
        let mut preview_cell = None;
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
            if self.button.get().is_none()
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
        let mut level = sim.world.resource_mut::<LevelDocument>();
        let mut world = self.world.borrow_mut();
        world.tick(&mut level, cam.target);
        world.pump_ghosts(dt_secs);
        let mut frame = Frame3d {
            cam,
            ..Frame3d::default()
        };
        frame.push(ground());
        world.draw(&mut frame, &level, preview_cell, in_edit);

        let name = level.data.name.clone();
        let blocks = level.map.len();
        let chunks = world.chunk_count();
        let action = world.last_action.clone();
        drop(world);
        drop(level);
        let mut play_lines = Vec::new();
        if mode == MakerMode::Play {
            let ui = sim.world.resource::<win::MakerUi>();
            play_lines.push(line(format!(
                "time {}",
                win::fmt_ms((ui.play_timer * 1000.0).round() as u32)
            )));
            if !ui.status.is_empty() {
                play_lines.push(line(ui.status.clone()));
            }
            if ui.sign_dialog_open {
                for sign_line in &ui.sign_dialog_lines {
                    play_lines.push(line(sign_line.clone()));
                }
            }
        }
        if mode == MakerMode::Play
            && let (Some(p), Some(tf), Some(ms)) = (
                sim.world.get::<Player>(self.player),
                sim.world.get::<PlayerTransform>(self.player),
                sim.world.get::<MoveState>(self.player),
            )
        {
            if tf.visible {
                frame.push(player_box(p, tf));
            }
            play_lines.push(line(format!(
                "pos ({:.2}, {:.2}, {:.2}) · {}",
                tf.translation.x,
                tf.translation.y,
                tf.translation.z,
                if ms.grounded { "grounded" } else { "air" }
            )));
        }
        draw_props(&sim.world, &mut frame);
        drop(sim);

        self.hotkeys(sched, &edges);

        let cam_rc = self.cam.clone();
        let world_rc = self.world.clone();
        let sim_rc = self.sim.clone();
        let button_rc = self.button.clone();
        let ctrl_rc = self.ctrl.clone();
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
                    View3dEvent::Drag { button, dx, dy } => {
                        if in_edit && !ui_wants_pointer && !ctrl {
                            match button {
                                PointerButton::Secondary => {
                                    c.yaw += dx * 0.005;
                                    c.pitch = (c.pitch + dy * 0.005).clamp(0.05, 1.5);
                                    if let Ok(mut sim) = sim_rc.try_borrow_mut() {
                                        let ray = sim.world.resource::<EditorCursor>().ray;
                                        let mirror = sim.world.resource::<MirrorMode>().0;
                                        let hit = ray.and_then(|(eye, dir)| {
                                            raycast_present(
                                                sim.world.resource::<LevelDocument>(),
                                                Vec3::from_array(eye),
                                                Vec3::from_array(dir),
                                                200.0,
                                            )
                                        });
                                        if let Some((cell, _)) = hit
                                            && let Ok(mut world) = world_rc.try_borrow_mut()
                                        {
                                            let mut level =
                                                sim.world.resource_mut::<LevelDocument>();
                                            world.stroke_erase(&mut level, mirror, cell);
                                        }
                                    }
                                }
                                PointerButton::Primary => {
                                    if let Ok(mut sim) = sim_rc.try_borrow_mut() {
                                        let ray = sim.world.resource::<EditorCursor>().ray;
                                        let mirror = sim.world.resource::<MirrorMode>().0;
                                        let hit = ray.and_then(|(eye, dir)| {
                                            raycast_present(
                                                sim.world.resource::<LevelDocument>(),
                                                Vec3::from_array(eye),
                                                Vec3::from_array(dir),
                                                200.0,
                                            )
                                        });
                                        if let Some((cell, normal)) = hit
                                            && let Ok(mut world) = world_rc.try_borrow_mut()
                                        {
                                            let mut level =
                                                sim.world.resource_mut::<LevelDocument>();
                                            world.stroke_paint(&mut level, mirror, cell + normal);
                                        }
                                    }
                                }
                                PointerButton::Tertiary => {}
                            }
                        }
                    }
                    click @ (View3dEvent::GroundClick { .. } | View3dEvent::MeshClick { .. }) => {
                        let button = button_rc.get();
                        let erase = matches!(button, Some(PointerButton::Secondary));
                        let place = matches!(button, Some(PointerButton::Primary));
                        if in_edit && !ui_wants_pointer && !ctrl && (erase || place) {
                            let Ok(mut sim) = sim_rc.try_borrow_mut() else {
                                return;
                            };
                            let mirror = sim.world.resource::<MirrorMode>().0;
                            let Ok(mut world) = world_rc.try_borrow_mut() else {
                                return;
                            };
                            let mut level = sim.world.resource_mut::<LevelDocument>();
                            world.click(&mut level, &click, &c, erase, mirror);
                        }
                    }
                }
                cam_rc.set(c);
            },
        );

        let mut hud_children = vec![
            line(format!("Rustbox · tick {elapsed:.2}s")),
            line(format!(
                "mode: {}",
                if mode == MakerMode::Edit {
                    "edit"
                } else {
                    "play"
                }
            )),
            line(format!("{name} · {blocks} blocks · {chunks} chunks")),
            line("left place · right erase · ctrl+z undo · ctrl+shift+z redo"),
            line(action),
        ];
        hud_children.extend(play_lines);
        let hud = Column(
            Modifier::new()
                .absolute()
                .offset(Some(Dp(16.0)), Some(Dp(16.0)), None, None)
                .hit_passthrough(),
        )
        .child(hud_children);

        let focus = remember(FocusRequester::new);
        let fr_positioned = (*focus).clone();
        let focus_staging = self.staging.clone();
        let key_staging = self.staging.clone();
        let button = self.button.clone();
        let button_up = self.button.clone();
        let button_cancel = self.button.clone();
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
                    key_staging.borrow_mut().handle_key(&ke);
                    false
                })
                .on_pointer_down(move |ev: PointerEvent| {
                    if let PointerEventKind::Down(b) = ev.event {
                        button.set(Some(b));
                    }
                })
                .on_pointer_up(move |ev: PointerEvent| {
                    if matches!(ev.event, PointerEventKind::Up(_)) {
                        button_up.set(None);
                    }
                })
                .on_pointer_cancel(move |_| {
                    button_cancel.set(None);
                }),
        )
        .child(vec![viewport, hud])
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

    fn hotkeys(&mut self, sched: &mut Scheduler, edges: &[PhysicalKey]) {
        {
            let sim = self.sim.borrow();
            if *sim.world.resource::<MakerMode>() != MakerMode::Edit {
                return;
            }
        }
        let held = |key: PhysicalKey| sched.held_keys.contains(&key);
        let ctrl = held(PhysicalKey::ControlLeft) || held(PhysicalKey::ControlRight);
        if edges.contains(&PhysicalKey::KeyV) && !ctrl {
            let Ok(mut sim) = self.sim.try_borrow_mut() else {
                return;
            };
            if !sim.world.resource::<InputCapture>().ui_wants_keyboard {
                let label = {
                    let mut mirror = sim.world.resource_mut::<MirrorMode>();
                    mirror.0 = (mirror.0 + 1) % 4;
                    match mirror.0 {
                        0 => "Off",
                        1 => "X",
                        2 => "Z",
                        _ => "X+Z",
                    }
                    .to_string()
                };
                sim.world
                    .resource_mut::<win::MakerUi>()
                    .set_status(format!("Mirror: {label}"));
                if let Ok(mut world) = self.world.try_borrow_mut() {
                    world.last_action = format!("Mirror: {label}");
                }
            }
        }
        let shift = held(PhysicalKey::ShiftLeft) || held(PhysicalKey::ShiftRight);
        let z = held(PhysicalKey::KeyZ);
        let y = held(PhysicalKey::KeyY);
        let undo = ctrl && z && !self.undo_latched;
        let redo = ctrl && y && !self.redo_latched;
        self.undo_latched = ctrl && z;
        self.redo_latched = ctrl && y;
        if !undo && !redo {
            return;
        }
        let Ok(mut sim) = self.sim.try_borrow_mut() else {
            return;
        };
        let Ok(mut world) = self.world.try_borrow_mut() else {
            return;
        };
        let mut level = sim.world.resource_mut::<LevelDocument>();
        if redo || (undo && shift) {
            world.redo(&mut level);
        } else {
            world.undo(&mut level);
        }
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

fn line(text: impl Into<String>) -> View {
    Text(text.into())
        .size(Sp(16.0))
        .color(Color::from_rgba(255, 255, 255, 255))
        .single_line()
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
