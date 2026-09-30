use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use repame_shell::{Sim, SimTime};
use repame_view3d::{
    BatchDesc, Frame3d, GeomHandle, MeshGroup, OrbitCamera, View3dEvent, Viewport3d,
};
use repose_core::{Color, Dp, Modifier, RenderContext, Scheduler, Sp, View, request_frame};
use repose_ui::{Column, Text, TextStyle, ViewExt, ZStack};
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

struct App {
    sim: Sim,
    cam: Rc<Cell<OrbitCamera>>,
    ground: MeshGroup,
    last: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            sim: Sim::with_default_step(),
            cam: Rc::new(Cell::new(OrbitCamera {
                target: Default::default(),
                yaw: -0.7,
                pitch: 0.7,
                dist: 34.0,
                fov_y_deg: 45.0,
            })),
            ground: ground(),
            last: Instant::now(),
        }
    }

    fn view(&mut self, _sched: &mut Scheduler, _ctx: &RenderContext) -> View {
        request_frame();
        let now = Instant::now();
        let dt = now
            .duration_since(self.last)
            .min(Duration::from_secs_f32(0.25));
        self.last = now;
        self.sim.step(dt);

        let mut frame = Frame3d {
            cam: self.cam.get(),
            ..Frame3d::default()
        };
        frame.push(self.ground.clone());

        let cam = self.cam.clone();
        let viewport = Viewport3d(
            frame,
            GeomHandle::new(),
            "scene.main",
            BatchDesc::default(),
            move |ev| {
                let mut c = cam.get();
                match ev {
                    View3dEvent::Orbit { dx, dy } => c.orbit(dx, dy),
                    View3dEvent::Pan { dx, dy } => c.pan(dx, dy),
                    View3dEvent::Zoom { factor } => c.zoom(factor),
                    _ => {}
                }
                cam.set(c);
            },
        );

        let elapsed = self.sim.world.resource::<SimTime>().elapsed_secs;
        let hud = Column(
            Modifier::new()
                .absolute()
                .offset(Some(Dp(16.0)), Some(Dp(16.0)), None, None)
                .hit_passthrough(),
        )
        .child(
            Text(format!("Rustbox · tick {elapsed:.2}s"))
                .size(Sp(16.0))
                .color(Color::from_rgba(255, 255, 255, 255))
                .single_line(),
        );

        ZStack(Modifier::new().fill_max_size()).child(vec![viewport, hud])
    }
}

fn ground() -> MeshGroup {
    let mut group = MeshGroup {
        depth_test: true,
        ..MeshGroup::default()
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
