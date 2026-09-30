pub mod maker;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use glam::Vec3;
use maker::level::LevelDocument;
use maker::level_file::deserialize_level;
use maker::level_view::LevelView;
use repame_shell::{Sim, SimTime};
use repame_view3d::{BatchDesc, Frame3d, GeomHandle, OrbitCamera, View3dEvent, Viewport3d};
use repose_core::input::{PhysicalKey, PointerButton, PointerEvent, PointerEventKind};
use repose_core::{Color, Dp, Modifier, RenderContext, Scheduler, Sp, View, request_frame};
use repose_ui::{Column, Text, TextStyle, ViewExt, ZStack};
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const BUNDLED_LEVEL: &str = include_str!("../assets/levels/01_first_steps.ron");

struct App {
    sim: Sim,
    cam: Rc<Cell<OrbitCamera>>,
    world: Rc<RefCell<LevelView>>,
    button: Rc<Cell<Option<PointerButton>>>,
    undo_latched: bool,
    redo_latched: bool,
    last: Instant,
}

impl App {
    fn new() -> Self {
        let data = deserialize_level(BUNDLED_LEVEL).expect("bundled level parses");
        let mut level = LevelDocument::default();
        level.replace_data(data);
        Self {
            sim: Sim::with_default_step(),
            cam: Rc::new(Cell::new(OrbitCamera {
                target: Vec3::new(0.5, 1.0, 0.0),
                yaw: -0.7,
                pitch: 0.62,
                dist: 16.0,
                fov_y_deg: 45.0,
            })),
            world: Rc::new(RefCell::new(LevelView::new(level))),
            button: Rc::new(Cell::new(None)),
            undo_latched: false,
            redo_latched: false,
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
        self.sim.step(dt);

        let cam = self.cam.get();
        let mut world = self.world.borrow_mut();
        world.tick(cam.target);
        let mut frame = Frame3d {
            cam,
            ..Frame3d::default()
        };
        frame.push(ground());
        world.draw(&mut frame);

        let name = world.name().to_string();
        let blocks = world.block_count();
        let chunks = world.chunk_count();
        let action = world.last_action.clone();
        drop(world);

        self.hotkeys(sched);

        let cam_rc = self.cam.clone();
        let world_rc = self.world.clone();
        let button_rc = self.button.clone();
        let viewport = Viewport3d(
            frame,
            GeomHandle::new(),
            "scene.main",
            BatchDesc::default(),
            move |ev| {
                let mut c = cam_rc.get();
                match ev {
                    View3dEvent::Orbit { dx, dy } => c.orbit(dx, dy),
                    View3dEvent::Pan { dx, dy } => c.pan(dx, dy),
                    View3dEvent::Zoom { factor } => c.zoom(factor),
                    click => {
                        let erase = matches!(button_rc.get(), Some(PointerButton::Secondary));
                        if let Ok(mut world) = world_rc.try_borrow_mut() {
                            world.click(&click, &c, erase);
                        }
                    }
                }
                cam_rc.set(c);
            },
        );

        let elapsed = self.sim.world.resource::<SimTime>().elapsed_secs;
        let hud = Column(
            Modifier::new()
                .absolute()
                .offset(Some(Dp(16.0)), Some(Dp(16.0)), None, None)
                .hit_passthrough(),
        )
        .child(vec![
            line(format!("Rustbox · tick {elapsed:.2}s")),
            line(format!("{name} · {blocks} blocks · {chunks} chunks")),
            line("left place · right erase · ctrl+z undo · ctrl+shift+z redo"),
            line(action),
        ]);

        let button = self.button.clone();
        ZStack(
            Modifier::new()
                .fill_max_size()
                .on_pointer_down(move |ev: PointerEvent| {
                    if let PointerEventKind::Down(b) = ev.event {
                        button.set(Some(b));
                    }
                }),
        )
        .child(vec![viewport, hud])
    }

    fn hotkeys(&mut self, sched: &mut Scheduler) {
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
        let Ok(mut world) = self.world.try_borrow_mut() else {
            return;
        };
        if redo || (undo && shift) {
            world.redo();
        } else {
            world.undo();
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
