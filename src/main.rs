//! Synthetic Sylvaculture — reproduction of
//! Makowski et al. 2019, "Synthetic Silviculture: Multi-scale Modeling of Plant Ecosystems".
//!
//! Milestone 1: a single plant grows in a native 3D window. Growth is driven by
//! the extended Borchert-Honda vigor distribution (apical control λ), module
//! development (Eqs. 5-10), and Pipe-Model thickening.
//!
//! Controls:  Space play/pause · S step · R reset · ←/→ apical control λ ·
//!            ↑/↓ plant growth rate · mouse orbit/zoom.

mod ecosystem;
mod mesh;
mod plant;
mod prototype;
mod species;

use ecosystem::Ecosystem;
use glam::vec3 as gvec3;
use plant::{Plant, PlantParams};
use three_d::*;

fn make_plant(params: PlantParams) -> Plant {
    Plant::new(prototype::default_library(), params, gvec3(0.0, 0.0, 0.0))
}

fn make_bark(context: &Context, rgb: (u8, u8, u8)) -> PhysicalMaterial {
    let mut m = PhysicalMaterial::new_opaque(
        context,
        &CpuMaterial {
            albedo: Srgba::new(rgb.0, rgb.1, rgb.2, 255),
            roughness: 0.9,
            metallic: 0.0,
            ..Default::default()
        },
    );
    m.render_states.cull = Cull::None;
    m
}

fn make_leaf(context: &Context, rgb: (u8, u8, u8)) -> PhysicalMaterial {
    let mut m = PhysicalMaterial::new_opaque(
        context,
        &CpuMaterial {
            albedo: Srgba::new(rgb.0, rgb.1, rgb.2, 255),
            roughness: 0.8,
            metallic: 0.0,
            ..Default::default()
        },
    );
    m.render_states.cull = Cull::None;
    m
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
                    "  λ={lambda:.2}  step {:>3}  modules {:>4}  segs {:>5}  leaves {:>4}  height {:.2}",
                    s + 1,
                    plant.module_count(),
                    segs.len(),
                    plant.leaves().len(),
                    plant.height()
                );
            }
        }
    }

    println!("\nspecies presets (100 steps each):");
    for sp in species::library() {
        let mut plant = make_plant(sp.params);
        for _ in 0..100 {
            plant.step(1.0);
        }
        println!(
            "  {:<22}  modules {:>4}  leaves {:>4}  height {:.2}",
            sp.name,
            plant.module_count(),
            plant.leaves().len(),
            plant.height()
        );
    }

    println!("\nflicker: back-and-forth wiggle of mature modules (path−net over 30 steps), λ=0.30:");
    for (label, committed) in [("committed", true), ("fixed", false)] {
        let mut params = PlantParams::default();
        params.lambda = 0.30;
        params.collision_light = true;
        params.optimize_orientation = true;
        if committed {
            // emulate the committed milestone-2 optimizer: undamped, no freeze.
            params.opt_damping = 1.0;
            params.opt_freeze_settled = false;
        }
        let mut plant = make_plant(params);
        for _ in 0..60 {
            plant.step(1.0);
        }
        let start = plant.mature_centroids();
        let mut last = start.clone();
        let mut path: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for _ in 0..30 {
            plant.step(1.0);
            let now = plant.mature_centroids();
            for (id, p0) in &last {
                if let Some(p1) = now.get(id) {
                    *path.entry(*id).or_insert(0.0) += (*p1 - *p0).length();
                }
            }
            last = now;
        }
        // wiggle = path travelled minus net displacement (pure oscillation).
        let mut wig = 0.0f32;
        let mut n = 0u32;
        for (id, &p) in &path {
            if let (Some(s), Some(e)) = (start.get(id), last.get(id)) {
                wig += p - (*e - *s).length();
                n += 1;
            }
        }
        println!(
            "  {label:<9}  avg wiggle {:.4} units over 30 steps",
            if n > 0 { wig / n as f32 } else { 0.0 }
        );
    }

    println!("\norientation optimization vs naive (Fig. 15a metric, λ=0.5, 120 steps):");
    for (label, opt) in [("naive", false), ("optimized", true)] {
        let mut params = PlantParams::default();
        params.lambda = 0.5;
        params.collision_light = opt;
        params.optimize_orientation = opt;
        let mut plant = make_plant(params);
        for _ in 0..120 {
            plant.step(1.0);
        }
        println!(
            "  {:<9}  modules {:>4}  intersection-volume ratio {:>6.1}%  height {:.2}",
            label,
            plant.module_count(),
            100.0 * plant.intersection_ratio(),
            plant.height()
        );
    }
}

/// Ecosystem viewer: a stand of mixed-species plants growing together on flat
/// ground, rendered as one combined per-species-coloured mesh. `--eco`.
fn run_ecosystem() {
    let window = Window::new(WindowSettings {
        title: "Synthetic Sylvaculture — Ecosystem".to_string(),
        max_size: Some((1400, 860)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let mut camera = Camera::new_perspective(
        window.viewport(),
        vec3(40.0, 26.0, 40.0),
        vec3(0.0, 6.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        2000.0,
    );
    let mut control = OrbitControl::new(camera.target(), 3.0, 400.0);

    let ambient = AmbientLight::new(&context, 0.5, Srgba::new(200, 215, 255, 255));
    let key = DirectionalLight::new(&context, 2.6, Srgba::new(255, 247, 230, 255), vec3(-0.5, -1.0, -0.7));
    let fill = DirectionalLight::new(&context, 0.9, Srgba::new(180, 200, 255, 255), vec3(0.8, -0.4, 0.5));

    let eco_size = 18.0f32;
    let mut seed = 7u64;
    let mut eco = Ecosystem::new(28, eco_size, seed);

    let mut ground = Gm::new(
        Mesh::new(&context, &CpuMesh::square()),
        PhysicalMaterial::new_opaque(
            &context,
            &CpuMaterial {
                albedo: Srgba::new(70, 105, 58, 255),
                ..Default::default()
            },
        ),
    );
    ground.set_transformation(Mat4::from_angle_x(degrees(-90.0)) * Mat4::from_scale(eco_size * 2.2));

    // White-albedo materials; all colour comes from per-vertex species tints.
    let mut wood = PhysicalMaterial::new_opaque(
        &context,
        &CpuMaterial { albedo: Srgba::WHITE, roughness: 0.9, metallic: 0.0, ..Default::default() },
    );
    wood.render_states.cull = Cull::None;
    let mut leaf = PhysicalMaterial::new_opaque(
        &context,
        &CpuMaterial { albedo: Srgba::WHITE, roughness: 0.8, metallic: 0.0, ..Default::default() },
    );
    leaf.render_states.cull = Cull::None;

    let leaf_size = 0.4;
    let per_cluster = 5;
    let build_trunks =
        |eco: &Ecosystem, ctx: &Context| Mesh::new(ctx, &mesh::build_forest_mesh(&eco.trunk_batches(), 6));
    let build_foliage = move |eco: &Ecosystem, ctx: &Context| {
        Mesh::new(ctx, &mesh::build_forest_foliage(&eco.foliage_batches(), leaf_size, per_cluster))
    };

    let mut trunks = Gm::new(build_trunks(&eco, &context), wood.clone());
    let mut foliage = Gm::new(build_foliage(&eco, &context), leaf.clone());

    let mut playing = true;
    let mut step_count: u32 = 0;
    let step_interval_ms = 120.0;
    let mut accum_ms = 0.0f64;
    let mut show_foliage = true;

    println!("Synthetic Sylvaculture — Ecosystem");
    println!("  Space play/pause · S step · R reseed · F foliage · mouse orbit/zoom");
    println!("  {} plants on a {}×{} ground", eco.plant_count(), (eco_size * 2.0) as i32, (eco_size * 2.0) as i32);

    window.render_loop(move |mut frame_input| {
        camera.set_viewport(frame_input.viewport);
        control.handle_events(&mut camera, &mut frame_input.events);

        let mut dirty = false;
        let mut reset = false;
        for event in frame_input.events.iter() {
            if let Event::KeyPress { kind, .. } = event {
                match kind {
                    Key::Space => playing = !playing,
                    Key::S => {
                        eco.step(1.0);
                        step_count += 1;
                        dirty = true;
                    }
                    Key::R => reset = true,
                    Key::F => {
                        show_foliage = !show_foliage;
                    }
                    _ => {}
                }
            }
        }

        if reset {
            seed += 1;
            eco = Ecosystem::new(28, eco_size, seed);
            step_count = 0;
            accum_ms = 0.0;
            dirty = true;
            println!("[reseed {seed}]");
        }

        if playing {
            accum_ms += frame_input.elapsed_time;
            while accum_ms >= step_interval_ms {
                accum_ms -= step_interval_ms;
                eco.step(1.0);
                step_count += 1;
                dirty = true;
            }
        }

        if dirty {
            trunks.geometry = build_trunks(&eco, &context);
            foliage.geometry = build_foliage(&eco, &context);
            if step_count % 10 == 0 || !playing {
                println!(
                    "  step {:>4}  plants {:>3}  modules {:>5}",
                    step_count,
                    eco.plant_count(),
                    eco.total_modules()
                );
            }
        }

        let screen = frame_input.screen();
        screen.clear(ClearState::color_and_depth(0.62, 0.74, 0.90, 1.0, 1.0));
        if show_foliage {
            screen.render(&camera, ground.into_iter().chain(&trunks).chain(&foliage), &[&ambient, &key, &fill]);
        } else {
            screen.render(&camera, ground.into_iter().chain(&trunks), &[&ambient, &key, &fill]);
        }

        FrameOutput::default()
    });
}

fn main() {
    if std::env::args().any(|a| a == "--stats") {
        run_stats();
        return;
    }
    if std::env::args().any(|a| a == "--eco") {
        run_ecosystem();
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

    let ambient = AmbientLight::new(&context, 0.45, Srgba::new(200, 215, 255, 255));
    let key = DirectionalLight::new(&context, 2.6, Srgba::new(255, 247, 230, 255), vec3(-0.5, -1.0, -0.7));
    let fill = DirectionalLight::new(&context, 0.9, Srgba::new(180, 200, 255, 255), vec3(0.8, -0.4, 0.5));

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

    // --- species presets (Sec. 6.1 / Tab. 4, adapted) ---
    let species = species::library();
    let mut sp_idx = 2usize; // birch
    let mut params = species[sp_idx].params.clone();

    let mut bark = make_bark(&context, species[sp_idx].bark_rgb);
    let mut leaf_mat = make_leaf(&context, species[sp_idx].leaf_rgb);

    let leaf_size = 0.38;
    let leaves_per_cluster = 5;
    let mut show_foliage = true;

    // --- simulation state ---
    let mut plant = make_plant(params.clone());
    let mut tree = Gm::new(
        Mesh::new(&context, &mesh::build_tree_mesh(&plant.skeleton(), 8)),
        bark.clone(),
    );
    let mut foliage = Gm::new(
        Mesh::new(
            &context,
            &mesh::build_foliage_mesh(&plant.leaves(), leaf_size, leaves_per_cluster),
        ),
        leaf_mat.clone(),
    );

    let mut playing = true;
    let mut step_count: u32 = 0;
    let step_interval_ms = 90.0; // sim step cadence while playing
    let mut accum_ms = 0.0f64;

    let rebuild = |plant: &Plant, context: &Context| {
        Mesh::new(context, &mesh::build_tree_mesh(&plant.skeleton(), 8))
    };
    let rebuild_foliage = move |plant: &Plant, context: &Context| {
        Mesh::new(
            context,
            &mesh::build_foliage_mesh(&plant.leaves(), leaf_size, leaves_per_cluster),
        )
    };

    println!("Synthetic Sylvaculture — single plant growth");
    println!("  Space play/pause · S step · R reset · ←/→ λ · ↑/↓ growth rate");
    println!("  N next species · O orientation-opt · L collision-light · F foliage");
    println!("  start species: {}  (λ={:.2}, D={:.2})", species[sp_idx].name, params.lambda, params.determinacy);

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
                    Key::O => {
                        params.optimize_orientation = !params.optimize_orientation;
                        reset = true;
                    }
                    Key::L => {
                        params.collision_light = !params.collision_light;
                        reset = true;
                    }
                    Key::F => {
                        show_foliage = !show_foliage;
                        println!("[foliage {}]", if show_foliage { "on" } else { "off" });
                    }
                    Key::N => {
                        sp_idx = (sp_idx + 1) % species.len();
                        params = species[sp_idx].params.clone();
                        bark = make_bark(&context, species[sp_idx].bark_rgb);
                        leaf_mat = make_leaf(&context, species[sp_idx].leaf_rgb);
                        tree.material = bark.clone();
                        foliage.material = leaf_mat.clone();
                        reset = true;
                        println!("[species] {}", species[sp_idx].name);
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
                "[reset] λ={:.2}  gp={:.2}  orient-opt={}  collision-light={}  ({})",
                params.lambda,
                params.gp,
                params.optimize_orientation,
                params.collision_light,
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
            foliage.geometry = rebuild_foliage(&plant, &context);
            if step_count % 10 == 0 || !playing {
                println!(
                    "  step {:>4}  modules {:>4}  age {:.1}",
                    step_count,
                    plant.module_count(),
                    plant.age
                );
            }
        }

        let screen = frame_input
            .screen();
        screen.clear(ClearState::color_and_depth(0.62, 0.74, 0.90, 1.0, 1.0));
        if show_foliage {
            screen.render(
                &camera,
                ground.into_iter().chain(&tree).chain(&foliage),
                &[&ambient, &key, &fill],
            );
        } else {
            screen.render(
                &camera,
                ground.into_iter().chain(&tree),
                &[&ambient, &key, &fill],
            );
        }

        FrameOutput::default()
    });
}
