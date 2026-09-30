pub mod maker;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::IntoScheduleConfigs;
use glam::Vec3;
use maker::camera::{CameraRig, play_camera_follow};
use maker::interactive_blocks::{OnOffState, reset_onoff_state};
use maker::level::LevelDocument;
use maker::level_file::deserialize_level;
use maker::level_view::LevelView;
use maker::mode::{InputCapture, MakerMode};
use maker::player::{
    MoveState, MoveTuning, PlayIntent, PlayKeys, Player, PlayerTransform, PressedLatch, Trauma,
    clear_pressed_latch, latch_play_presses, player_controller, spawn_player, sync_mode,
};
use maker::props::{ActivePlates, RuntimeSolids};
use maker::{Paused, interaction, not_paused, win};
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
        let player = spawn_player(&mut sim.world, &level);
        sim.world.insert_resource(level);
        sim.world.insert_resource(MakerMode::Edit);
        sim.world.insert_resource(InputCapture::default());
        sim.world.insert_resource(PlayIntent::default());
        sim.world.insert_resource(PressedLatch::default());
        sim.world.insert_resource(MoveTuning::default());
        sim.world.insert_resource(RuntimeSolids::default());
        sim.world.insert_resource(OnOffState::default());
        sim.world.insert_resource(Trauma::default());
        sim.world.insert_resource(ActivePlates::default());
        sim.world.insert_resource(CameraRig::default());
        sim.world.insert_resource(Paused(false));
        sim.world.insert_resource(win::MakerUi::default());
        sim.add_chained_systems(
            (
                sync_mode,
                reset_onoff_state,
                player_controller,
                clear_pressed_latch,
            )
                .chain()
                .run_if(not_paused),
        );
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
            win::on_mode_changed(&mut sim.world, self.prev_mode, mode);
            self.prev_mode = mode;
        }
        win::retry_play(&mut sim.world, mode);
        sim.step(dt);
        let elapsed = sim.world.resource::<SimTime>().elapsed_secs;
        let dt_secs = dt.as_secs_f32();
        interaction::play_hazard_goal(&mut sim.world);
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
        let mut level = sim.world.resource_mut::<LevelDocument>();
        let mut world = self.world.borrow_mut();
        world.tick(&mut level, cam.target);
        let mut frame = Frame3d {
            cam,
            ..Frame3d::default()
        };
        frame.push(ground());
        world.draw(&mut frame);

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
        drop(sim);

        self.hotkeys(sched);

        let cam_rc = self.cam.clone();
        let world_rc = self.world.clone();
        let sim_rc = self.sim.clone();
        let button_rc = self.button.clone();
        let looked_rc = self.looked.clone();
        let viewport = Viewport3d(
            frame,
            GeomHandle::new(),
            "scene.main",
            BatchDesc::default(),
            move |ev| {
                let mut c = cam_rc.get();
                let (play_live, in_edit) = {
                    let Ok(sim) = sim_rc.try_borrow() else {
                        return;
                    };
                    let m = *sim.world.resource::<MakerMode>();
                    let paused = sim.world.resource::<Paused>().0;
                    (m == MakerMode::Play && !paused, m == MakerMode::Edit)
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
                        } else if in_edit {
                            c.orbit(dx, dy);
                        }
                    }
                    View3dEvent::Pan { dx, dy } => {
                        if in_edit {
                            c.pan(dx, dy);
                        }
                    }
                    View3dEvent::Zoom { factor } => {
                        if play_live {
                            let scroll = (1.0 - factor) / 0.002;
                            let Ok(mut sim) = sim_rc.try_borrow_mut() else {
                                return;
                            };
                            let mut rig = sim.world.resource_mut::<CameraRig>();
                            rig.distance = (rig.distance - scroll * 0.9).clamp(5.0, 22.0);
                        } else if in_edit {
                            c.zoom(factor);
                        }
                    }
                    click => {
                        let erase = matches!(button_rc.get(), Some(PointerButton::Secondary));
                        let Ok(mut sim) = sim_rc.try_borrow_mut() else {
                            return;
                        };
                        if *sim.world.resource::<MakerMode>() != MakerMode::Edit {
                            return;
                        }
                        let Ok(mut world) = world_rc.try_borrow_mut() else {
                            return;
                        };
                        let mut level = sim.world.resource_mut::<LevelDocument>();
                        world.click(&mut level, &click, &c, erase);
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

    fn hotkeys(&mut self, sched: &mut Scheduler) {
        {
            let sim = self.sim.borrow();
            if *sim.world.resource::<MakerMode>() != MakerMode::Edit {
                return;
            }
        }
        let held = |key: PhysicalKey| sched.held_keys.contains(&key);
        let ctrl = held(PhysicalKey::ControlLeft) || held(PhysicalKey::ControlRight);
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
