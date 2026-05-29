//! Synthetic Sylvaculture — reproduction of
//! Makowski et al. 2019, "Synthetic Silviculture: Multi-scale Modeling of Plant Ecosystems".
//!
//! Milestone 1: a single plant grows in a native 3D window. Growth is driven by
//! the extended Borchert-Honda vigor distribution (apical control λ), module
//! development (Eqs. 5-10), and Pipe-Model thickening.
//!
//! Controls:  Space play/pause · S step · R reset · ←/→ apical control λ ·
//!            ↑/↓ plant growth rate · mouse orbit/zoom.

mod mesh;
mod plant;
mod prototype;

use glam::vec3 as gvec3;
use plant::{Plant, PlantParams};
use three_d::*;

fn make_plant(params: PlantParams) -> Plant {
    Plant::new(prototype::default_library(), params, gvec3(0.0, 0.0, 0.0))
}

/// Headless sanity/tuning sweep — no window, no GL. `cargo run -- --stats`.
fn run_stats() {
    println!("apical-control sweep (120 steps each):");
    for lambda in [0.2f32, 0.5, 0.72, 0.9] {
        let mut params = PlantParams::default();
        params.lambda = lambda;
        let mut plant = make_plant(params);
        for s in 0..120 {
            plant.step(1.0);
            if s % 40 == 39 {
                let segs = plant.skeleton();
                println!(
                    "  λ={lambda:.2}  step {:>3}  modules {:>4}  segs {:>5}  height {:.2}",
                    s + 1,
                    plant.module_count(),
                    segs.len(),
                    plant.height()
                );
            }
        }
    }
}

fn main() {
    if std::env::args().any(|a| a == "--stats") {
        run_stats();
        return;
    }

    let window = Window::new(WindowSettings {
        title: "Synthetic Sylvaculture".to_string(),
        max_size: Some((1280, 800)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let mut camera = Camera::new_perspective(
        window.viewport(),
        vec3(8.0, 5.0, 8.0),
        vec3(0.0, 3.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        1000.0,
    );
    let mut control = OrbitControl::new(camera.target(), 1.0, 100.0);

    let ambient = AmbientLight::new(&context, 0.5, Srgba::WHITE);
    let directional =
        DirectionalLight::new(&context, 2.5, Srgba::WHITE, vec3(-0.5, -1.0, -0.7));

    // Ground plane.
    let mut ground = Gm::new(
        Mesh::new(&context, &CpuMesh::square()),
        PhysicalMaterial::new_opaque(
            &context,
            &CpuMaterial {
                albedo: Srgba::new(70, 110, 60, 255),
                ..Default::default()
            },
        ),
    );
    ground.set_transformation(Mat4::from_angle_x(degrees(-90.0)) * Mat4::from_scale(50.0));

    // Bark material (cull disabled during bring-up so winding never hides it).
    let mut bark = PhysicalMaterial::new_opaque(
        &context,
        &CpuMaterial {
            albedo: Srgba::new(125, 86, 56, 255),
            roughness: 0.9,
            metallic: 0.0,
            ..Default::default()
        },
    );
    bark.render_states.cull = Cull::None;

    // --- simulation state ---
    let mut params = PlantParams::default();
    let mut plant = make_plant(params.clone());
    let mut tree = Gm::new(
        Mesh::new(&context, &mesh::build_tree_mesh(&plant.skeleton(), 8)),
        bark.clone(),
    );

    let mut playing = true;
    let mut step_count: u32 = 0;
    let step_interval_ms = 90.0; // sim step cadence while playing
    let mut accum_ms = 0.0f64;

    let rebuild = |plant: &Plant, context: &Context| {
        Mesh::new(context, &mesh::build_tree_mesh(&plant.skeleton(), 8))
    };

    println!("Synthetic Sylvaculture — single plant growth");
    println!("  Space play/pause · S step · R reset · ←/→ λ · ↑/↓ growth rate");
    println!(
        "  start: λ={:.2}  gp={:.2}  v_root_max={:.0}",
        params.lambda, params.gp, params.v_root_max
    );

    window.render_loop(move |mut frame_input| {
        camera.set_viewport(frame_input.viewport);
        control.handle_events(&mut camera, &mut frame_input.events);

        let mut dirty = false;
        let mut reset = false;
        for event in frame_input.events.iter() {
            if let Event::KeyPress { kind, .. } = event {
                match kind {
                    Key::Space => {
                        playing = !playing;
                        println!("[{}]", if playing { "playing" } else { "paused" });
                    }
                    Key::S => {
                        plant.step(1.0);
                        step_count += 1;
                        dirty = true;
                    }
                    Key::R => reset = true,
                    Key::ArrowLeft => {
                        params.lambda = (params.lambda - 0.05).clamp(0.05, 0.95);
                        reset = true;
                    }
                    Key::ArrowRight => {
                        params.lambda = (params.lambda + 0.05).clamp(0.05, 0.95);
                        reset = true;
                    }
                    Key::ArrowUp => {
                        params.gp = (params.gp + 0.05).clamp(0.05, 1.0);
                        reset = true;
                    }
                    Key::ArrowDown => {
                        params.gp = (params.gp - 0.05).clamp(0.05, 1.0);
                        reset = true;
                    }
                    _ => {}
                }
            }
        }

        if reset {
            plant = make_plant(params.clone());
            step_count = 0;
            accum_ms = 0.0;
            dirty = true;
            println!(
                "[reset] λ={:.2}  gp={:.2}  ({})",
                params.lambda,
                params.gp,
                if params.lambda > 0.5 {
                    "excurrent"
                } else {
                    "decurrent"
                }
            );
        }

        if playing {
            accum_ms += frame_input.elapsed_time;
            while accum_ms >= step_interval_ms {
                accum_ms -= step_interval_ms;
                plant.step(1.0);
                step_count += 1;
                dirty = true;
            }
        }

        if dirty {
            tree.geometry = rebuild(&plant, &context);
            if step_count % 10 == 0 || !playing {
                println!(
                    "  step {:>4}  modules {:>4}  age {:.1}",
                    step_count,
                    plant.module_count(),
                    plant.age
                );
            }
        }

        frame_input
            .screen()
            .clear(ClearState::color_and_depth(0.55, 0.68, 0.85, 1.0, 1.0))
            .render(
                &camera,
                ground.into_iter().chain(&tree),
                &[&ambient, &directional],
            );

        FrameOutput::default()
    });
}
