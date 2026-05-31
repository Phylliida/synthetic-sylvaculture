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
mod overlay;
mod plant;
mod prototype;
mod species;

use ecosystem::{biome_name, Climate, Ecosystem};
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

    let temperate = Climate { temp: 10.0, precip: 90.0 };

    println!("\necosystem: global shadowing on vs off (40 plants on a 26×26 plot, 140 steps):");
    for shadow in [false, true] {
        let mut eco = Ecosystem::new(40, 13.0, 7, temperate);
        eco.shadow_enabled = shadow;
        for _ in 0..140 {
            eco.step(1.0);
        }
        let mut h = eco.plant_heights();
        h.sort_by(f32::total_cmp);
        let med = h[h.len() / 2];
        println!(
            "  shadow {:<3}  plants {:>3}  total modules {:>5}  median height {:.1}  tallest {:.1}",
            if shadow { "on" } else { "off" },
            eco.plant_count(),
            eco.total_modules(),
            med,
            h.last().copied().unwrap_or(0.0),
        );
    }

    let names: Vec<&str> = Ecosystem::new(0, 1.0, 0, temperate)
        .species
        .iter()
        .map(|s| s.name)
        .collect();

    println!("\necosystem: succession (temperate, species counts over time):");
    {
        let mut eco = Ecosystem::new(30, 13.0, 4, temperate);
        for s in 1..=360 {
            eco.step(1.0);
            if [60, 150, 260, 360].contains(&s) {
                let counts = eco.species_counts();
                let comp: Vec<String> = counts
                    .iter()
                    .zip(&names)
                    .filter(|(c, _)| **c > 0)
                    .map(|(c, n)| format!("{n}:{c}"))
                    .collect();
                println!("  step {s:>3}  plants {:>3}  [{}]", eco.plant_count(), comp.join(", "));
            }
        }
    }

    println!("\necosystem: biome composition across climates (180 steps each):");
    for clim in [
        Climate { temp: -3.0, precip: 60.0 },
        Climate { temp: 10.0, precip: 90.0 },
        Climate { temp: 24.0, precip: 200.0 },
    ] {
        let mut eco = Ecosystem::new(36, 13.0, 9, clim);
        for _ in 0..180 {
            eco.step(1.0);
        }
        let counts = eco.species_counts();
        let dom = counts
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| **c)
            .map(|(i, _)| names[i])
            .unwrap_or("none");
        println!(
            "  T={:>4.0}°C P={:>3.0}cm  {:<28}  plants {:>3}  dominant: {}",
            clim.temp,
            clim.precip,
            biome_name(clim.temp, clim.precip),
            eco.plant_count(),
            dom
        );
    }

    println!("\ntree morphology (each species grown solo to ~70% of its lifespan):");
    for sp in species::library() {
        // Measure within the species' lifespan, not past senescence (e.g. the
        // short-lived shrub would otherwise be shown as a dying stump).
        let steps = ((0.7 * sp.params.p_max) as u32).min(150);
        let mut plant = make_plant(sp.params.clone());
        for _ in 0..steps {
            plant.step(1.0);
        }
        let segs = plant.skeleton();
        let basal = segs.iter().map(|s| s.ra).fold(0.0, f32::max);
        let (h, crown, apex) = plant.shape();
        println!(
            "  {:<22} mod {:>3}  h {:5.1}  trunk_r {:.3}  slender {:>4.0}  spread {:.2}  apex_lean {:.2}",
            sp.name,
            plant.module_count(),
            h,
            basal,
            h / (2.0 * basal).max(1e-3),
            crown / h.max(1e-3),
            apex / h.max(1e-3),
        );
    }

    println!("\nforest arc (boreal stand, 160 steps) — apex-lean of tall plants (lower = straighter):");
    {
        let mut eco = Ecosystem::new(40, 14.0, 7, Climate { temp: 5.0, precip: 80.0 });
        for _ in 0..160 {
            eco.step(1.0);
        }
        let leans: Vec<f32> = eco
            .plants
            .iter()
            .filter_map(|p| {
                let (h, _, apex) = p.shape();
                (h > 6.0).then_some(apex / h)
            })
            .collect();
        let n = leans.len().max(1);
        let mean = leans.iter().sum::<f32>() / n as f32;
        let max = leans.iter().cloned().fold(0.0, f32::max);
        println!("  tall plants {}  mean apex_lean {:.2}  max {:.2}", leans.len(), mean, max);
    }

    println!("\nforest mesh size (CPU build, no GL) — boreal vs temperate, 170 steps:");
    for (label, clim) in [
        ("boreal", Climate { temp: 5.0, precip: 85.0 }),
        ("temperate", Climate { temp: 12.0, precip: 130.0 }),
    ] {
        let mut eco = Ecosystem::new(40, 14.0, 7, clim);
        for _ in 0..170 {
            eco.step(1.0);
        }
        let trunk = mesh::build_forest_mesh(&eco.trunk_batches(), 6);
        let foliage = mesh::build_forest_foliage(&eco.foliage_batches(), 0.4, 5);
        let tv = match &trunk.positions {
            three_d::Positions::F32(v) => v.len(),
            _ => 0,
        };
        let fv = match &foliage.positions {
            three_d::Positions::F32(v) => v.len(),
            _ => 0,
        };
        let nan = match &trunk.positions {
            three_d::Positions::F32(v) => v.iter().any(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite()),
            _ => false,
        };
        println!(
            "  {label:<10} plants {:>3}  modules {:>5}  trunk_verts {:>7}  foliage_verts {:>7}  nan:{}",
            eco.plant_count(),
            eco.total_modules(),
            tv,
            fv,
            nan
        );
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

    println!("\norientation optimization vs naive (Fig. 15a metric, dense crown, 120 steps):");
    for (label, opt) in [("naive", false), ("optimized", true)] {
        // A deliberately dense, bushy crown (short segments, many modules) so
        // there are real collisions for the optimizer to resolve.
        let mut params = PlantParams::default();
        params.lambda = 0.5;
        params.l_max = 0.6;
        params.v_root_max = 120.0;
        params.v_max = 28.0;
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

/// Render one ecosystem frame (scene + biome-chart overlay) off-screen and
/// return the RGBA pixels (top-to-bottom). Lets us screenshot the viewer
/// without a visible window, so the rendered result can be inspected directly.
fn render_shot(context: &Context, eco: &Ecosystem, climate: Climate, w: u32, h: u32) -> Vec<[u8; 4]> {
    let color = Texture2D::new_empty::<[u8; 4]>(
        context,
        w,
        h,
        Interpolation::Linear,
        Interpolation::Linear,
        None,
        Wrapping::ClampToEdge,
        Wrapping::ClampToEdge,
    );
    let depth =
        DepthTexture2D::new::<f32>(context, w, h, Wrapping::ClampToEdge, Wrapping::ClampToEdge);
    let target = RenderTarget::new(color.as_color_target(None), depth.as_depth_target());
    let viewport = Viewport { x: 0, y: 0, width: w, height: h };

    let camera = Camera::new_perspective(
        viewport,
        vec3(40.0, 26.0, 40.0),
        vec3(0.0, 6.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        2000.0,
    );
    let ambient = AmbientLight::new(context, 0.5, Srgba::new(200, 215, 255, 255));
    let key = DirectionalLight::new(context, 2.6, Srgba::new(255, 247, 230, 255), vec3(-0.5, -1.0, -0.7));
    let fill = DirectionalLight::new(context, 0.9, Srgba::new(180, 200, 255, 255), vec3(0.8, -0.4, 0.5));

    let mut ground = Gm::new(
        Mesh::new(context, &CpuMesh::square()),
        PhysicalMaterial::new_opaque(context, &CpuMaterial { albedo: Srgba::new(70, 105, 58, 255), ..Default::default() }),
    );
    ground.set_transformation(Mat4::from_angle_x(degrees(-90.0)) * Mat4::from_scale(eco.size * 2.2));

    let mut wood = PhysicalMaterial::new_opaque(context, &CpuMaterial { albedo: Srgba::WHITE, roughness: 0.9, ..Default::default() });
    wood.render_states.cull = Cull::None;
    let mut leaf = PhysicalMaterial::new_opaque(context, &CpuMaterial { albedo: Srgba::WHITE, roughness: 0.8, ..Default::default() });
    leaf.render_states.cull = Cull::None;
    let trunks = Gm::new(Mesh::new(context, &mesh::build_forest_mesh(&eco.trunk_batches(), 6)), wood);
    let foliage = Gm::new(Mesh::new(context, &mesh::build_forest_foliage(&eco.foliage_batches(), 0.4, 5)), leaf);

    let mut overlay_mat = ColorMaterial::default();
    overlay_mat.render_states.cull = Cull::None;
    overlay_mat.render_states.depth_test = DepthTest::Always;
    let cam2d = Camera::new_2d(viewport);
    let chart = Gm::new(
        Mesh::new(context, &overlay::build_chart(viewport, climate.temp, climate.precip)),
        overlay_mat,
    );

    target.clear(ClearState::color_and_depth(0.62, 0.74, 0.90, 1.0, 1.0));
    target.render(&camera, ground.into_iter().chain(&trunks).chain(&foliage), &[&ambient, &key, &fill]);
    target.render(&cam2d, &chart, &[] as &[&dyn Light]);
    target.read_color::<[u8; 4]>()
}

/// `--shot <file.png> [--temp T] [--precip P] [--steps N]`: grow an ecosystem
/// and save a screenshot (needs a GL context, so it opens a window briefly).
fn run_shot() {
    let args: Vec<String> = std::env::args().collect();
    let val = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    let path = val("--shot").cloned().unwrap_or_else(|| "/tmp/shot.png".to_string());
    let temp: f32 = val("--temp").and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let precip: f32 = val("--precip").and_then(|s| s.parse().ok()).unwrap_or(90.0);
    let steps: u32 = val("--steps").and_then(|s| s.parse().ok()).unwrap_or(110);
    let (w, h) = (1280u32, 800u32);

    let window = Window::new(WindowSettings {
        title: "shot".to_string(),
        max_size: Some((w, h)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let climate = Climate { temp, precip };
    let mut eco = Ecosystem::new(40, 14.0, 7, climate);
    for _ in 0..steps {
        eco.step(1.0);
    }
    let pixels = render_shot(&context, &eco, climate, w, h);

    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = pixels[(y * w + x) as usize];
            img.put_pixel(x, y, image::Rgba(p));
        }
    }
    img.save(&path).unwrap();
    println!(
        "wrote {path}  ({:.0}°C {:.0}cm → {}, {} plants, {} steps)",
        temp,
        precip,
        biome_name(temp, precip),
        eco.plant_count(),
        steps
    );
    // The image is saved; exit before the winit/Wayland context teardown, which
    // segfaults on shutdown (proxies still attached) and otherwise looks like a
    // failed render even though the PNG is fine.
    std::io::Write::flush(&mut std::io::stdout()).ok();
    std::process::exit(0);
}

/// `--tree <idx> [--steps N] [--shot file]`: grow ONE species solo and render
/// it large and centred to a PNG. The single-tree tuning instrument — the
/// ecosystem shot is too crowded to judge an individual tree's form.
fn run_tree_shot() {
    let args: Vec<String> = std::env::args().collect();
    let val = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    let idx: usize = val("--tree").and_then(|s| s.parse().ok()).unwrap_or(0);
    let steps: u32 = val("--steps").and_then(|s| s.parse().ok()).unwrap_or(120);
    let path = val("--shot").cloned().unwrap_or_else(|| "/tmp/tree.png".to_string());
    let (w, h) = (900u32, 1100u32);

    let window = Window::new(WindowSettings {
        title: "tree".to_string(),
        max_size: Some((w, h)),
        ..Default::default()
    })
    .unwrap();
    let context = window.gl();

    let species = species::library();
    let sp = &species[idx.min(species.len() - 1)];
    let mut plant = make_plant(sp.params.clone());
    for _ in 0..steps {
        plant.step(1.0);
    }
    let (height, crown, _) = plant.shape();
    let height = height.max(2.0);
    let reach = crown.max(height * 0.4).max(2.0);

    let color = Texture2D::new_empty::<[u8; 4]>(
        &context, w, h, Interpolation::Linear, Interpolation::Linear, None,
        Wrapping::ClampToEdge, Wrapping::ClampToEdge,
    );
    let depth = DepthTexture2D::new::<f32>(&context, w, h, Wrapping::ClampToEdge, Wrapping::ClampToEdge);
    let target = RenderTarget::new(color.as_color_target(None), depth.as_depth_target());
    let viewport = Viewport { x: 0, y: 0, width: w, height: h };

    // Frame the whole tree: pull the camera back proportional to its extent.
    let dist = (height.max(2.0 * reach)) * 1.5 + 4.0;
    let camera = Camera::new_perspective(
        viewport,
        vec3(dist, height * 0.5, dist),
        vec3(0.0, height * 0.5, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(40.0),
        0.1,
        2000.0,
    );
    let ambient = AmbientLight::new(&context, 0.5, Srgba::new(200, 215, 255, 255));
    let key = DirectionalLight::new(&context, 2.6, Srgba::new(255, 247, 230, 255), vec3(-0.5, -1.0, -0.7));
    let fill = DirectionalLight::new(&context, 0.9, Srgba::new(180, 200, 255, 255), vec3(0.8, -0.4, 0.5));

    let mut ground = Gm::new(
        Mesh::new(&context, &CpuMesh::square()),
        PhysicalMaterial::new_opaque(&context, &CpuMaterial { albedo: Srgba::new(70, 105, 58, 255), ..Default::default() }),
    );
    ground.set_transformation(Mat4::from_angle_x(degrees(-90.0)) * Mat4::from_scale(40.0));

    let tree = Gm::new(
        Mesh::new(&context, &mesh::build_tree_mesh(&plant.skeleton(), 10)),
        make_bark(&context, sp.bark_rgb),
    );
    let foliage = Gm::new(
        Mesh::new(&context, &mesh::build_foliage_mesh(&plant.leaves(), 0.4, 6)),
        make_leaf(&context, sp.leaf_rgb),
    );

    target.clear(ClearState::color_and_depth(0.62, 0.74, 0.90, 1.0, 1.0));
    target.render(&camera, ground.into_iter().chain(&tree).chain(&foliage), &[&ambient, &key, &fill]);
    let pixels = target.read_color::<[u8; 4]>();

    let mut img = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            img.put_pixel(x, y, image::Rgba(pixels[(y * w + x) as usize]));
        }
    }
    img.save(&path).unwrap();
    println!(
        "wrote {path}  ({}, {} steps → {} modules, h {:.1}, crown {:.1}, {} leaves)",
        sp.name, steps, plant.module_count(), height, crown, plant.leaves().len()
    );
    std::io::Write::flush(&mut std::io::stdout()).ok();
    std::process::exit(0);
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

    let eco_size = 14.0f32;
    let plant_count = 40;
    let mut seed = 7u64;
    let mut climate = Climate { temp: 10.0, precip: 90.0 };
    let mut eco = Ecosystem::new(plant_count, eco_size, seed, climate);

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

    // Unlit material for the 2D biome-chart overlay (vertex-coloured, on top).
    let mut overlay_mat = ColorMaterial::default();
    overlay_mat.render_states.cull = Cull::None;
    overlay_mat.render_states.depth_test = DepthTest::Always;

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
    println!("  ←/→ temperature · ↑/↓ precipitation · or CLICK the biome chart (top-left)");
    println!(
        "  climate: {:.0}°C, {:.0}cm  →  {}",
        climate.temp,
        climate.precip,
        biome_name(climate.temp, climate.precip)
    );

    window.render_loop(move |mut frame_input| {
        let vp = frame_input.viewport;
        camera.set_viewport(vp);

        let mut dirty = false;
        let mut reset = false;

        // Intercept clicks on the biome chart *before* the orbit control, so a
        // chart click sets the climate instead of spinning the camera.
        for event in frame_input.events.iter_mut() {
            if let Event::MousePress { button: MouseButton::Left, position, handled, .. } = event {
                if !*handled {
                    if let Some((t, p)) = overlay::screen_to_climate(vp, position.x, position.y) {
                        climate.temp = t;
                        climate.precip = p;
                        reset = true;
                        *handled = true;
                    }
                }
            }
        }

        control.handle_events(&mut camera, &mut frame_input.events);

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
                    Key::ArrowLeft => {
                        climate.temp = (climate.temp - 2.0).clamp(-10.0, 30.0);
                        reset = true;
                    }
                    Key::ArrowRight => {
                        climate.temp = (climate.temp + 2.0).clamp(-10.0, 30.0);
                        reset = true;
                    }
                    Key::ArrowDown => {
                        climate.precip = (climate.precip - 15.0).clamp(10.0, 400.0);
                        reset = true;
                    }
                    Key::ArrowUp => {
                        climate.precip = (climate.precip + 15.0).clamp(10.0, 400.0);
                        reset = true;
                    }
                    _ => {}
                }
            }
        }

        if reset {
            seed += 1;
            eco = Ecosystem::new(plant_count, eco_size, seed, climate);
            step_count = 0;
            accum_ms = 0.0;
            dirty = true;
            println!(
                "[reseed] {:.0}°C, {:.0}cm → {}",
                climate.temp,
                climate.precip,
                biome_name(climate.temp, climate.precip)
            );
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

        // 2D biome-chart overlay (drawn on top via DepthTest::Always).
        let cam2d = Camera::new_2d(vp);
        let chart = Gm::new(
            Mesh::new(&context, &overlay::build_chart(vp, climate.temp, climate.precip)),
            overlay_mat.clone(),
        );
        screen.render(&cam2d, &chart, &[] as &[&dyn Light]);

        FrameOutput::default()
    });
}

fn main() {
    if std::env::args().any(|a| a == "--stats") {
        run_stats();
        return;
    }
    if std::env::args().any(|a| a == "--tree") {
        run_tree_shot();
        return;
    }
    if std::env::args().any(|a| a == "--shot") {
        run_shot();
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

    let leaf_size = 0.38;
    let leaves_per_cluster = 5;
    let mut show_foliage = true;

    // --- simulation state ---
    let mut plant = make_plant(params.clone());
    let mut tree = Gm::new(
        Mesh::new(&context, &mesh::build_tree_mesh(&plant.skeleton(), 8)),
        make_bark(&context, species[sp_idx].bark_rgb),
    );
    let mut foliage = Gm::new(
        Mesh::new(
            &context,
            &mesh::build_foliage_mesh(&plant.leaves(), leaf_size, leaves_per_cluster),
        ),
        make_leaf(&context, species[sp_idx].leaf_rgb),
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
                        tree.material = make_bark(&context, species[sp_idx].bark_rgb);
                        foliage.material = make_leaf(&context, species[sp_idx].leaf_rgb);
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
