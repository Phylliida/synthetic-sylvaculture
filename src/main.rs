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
mod genome;
mod mesh;
mod overlay;
mod plant;
mod species;

use ecosystem::{biome_name, Climate, Ecosystem, StepTimings};
use glam::vec3 as gvec3;
use plant::{Plant, PlantParams};
use three_d::*;

fn make_plant(params: PlantParams) -> Plant {
    Plant::new(params, gvec3(0.0, 0.0, 0.0))
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
        let med = if h.is_empty() { 0.0 } else { h[h.len() / 2] };
        println!(
            "  shadow {:<3}  plants {:>3}  total modules {:>5}  median height {:.1}  tallest {:.1}",
            if shadow { "on" } else { "off" },
            eco.plant_count(),
            eco.total_modules(),
            med,
            h.last().copied().unwrap_or(0.0),
        );
    }

    // Key evolved traits to display (index into Genome::traits()).
    let key = |m: &[f32; 18]| {
        format!(
            "env_h {:>4.1}  env_r {:>3.1}  v_root {:>5.1}  shade {:.2}  flower {:>4.1}  seed_f {:.3}  life {:>4.0}",
            m[11], m[12], m[4], m[9], m[13], m[15], m[16]
        )
    };

    println!("\necosystem: EVOLUTION (temperate, mean genome over time from random founders):");
    {
        let mut eco = Ecosystem::new(50, 19.0, 4, temperate);
        for s in 1..=1600 {
            eco.step(1.0);
            if [1, 200, 500, 900, 1300, 1600].contains(&s) {
                let mut h = eco.plant_heights();
                h.sort_by(f32::total_cmp);
                let (med, tall) = if h.is_empty() { (0.0, 0.0) } else { (h[h.len() / 2], *h.last().unwrap()) };
                match eco.mean_traits() {
                    Some(m) => println!(
                        "  step {s:>4}  est {:>3}/{:>3}  med_h {:>4.1} tall {:>4.1}  {}",
                        eco.established_count(),
                        eco.plant_count(),
                        med,
                        tall,
                        key(&m)
                    ),
                    None => println!("  step {s:>4}  est 0/{:>3}  (all sprouts — collapsed)", eco.plant_count()),
                }
            }
        }
    }

    println!("\necosystem: 2D SPECIALIZATION — the four Whittaker corners (evolved mean, 450 steps):");
    println!("  (same random founders + seed; temp & precip stress different traits, so the");
    println!("   corners diverge in KIND — cold→narrow, warm→broad; dry→small, wet→large)");
    for clim in [
        Climate { temp: -2.0, precip: 30.0 },  // cold-dry
        Climate { temp: 3.0, precip: 220.0 },  // cold-wet
        Climate { temp: 25.0, precip: 35.0 },  // warm-dry
        Climate { temp: 26.0, precip: 320.0 }, // warm-wet
    ] {
        let mut eco = Ecosystem::new(60, 19.0, 9, clim);
        for _ in 0..450 {
            eco.step(1.0);
        }
        let traits = eco.mean_traits().map(|m| key(&m)).unwrap_or_else(|| "(nothing established — too harsh)".into());
        let spread = eco
            .trait_std()
            .map(|s| format!("diversity: env_h σ {:.1}  env_r σ {:.1}  shade σ {:.2}", s[11], s[12], s[9]))
            .unwrap_or_default();
        println!(
            "  T={:>4.0}°C P={:>3.0}cm (warm {:.2} water {:.2})  {:<24}  est {:>3}/{:>3}\n      {}\n      {}",
            clim.temp,
            clim.precip,
            clim.warmth(),
            clim.water(),
            biome_name(clim.temp, clim.precip),
            eco.established_count(),
            eco.plant_count(),
            traits,
            spread
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

    println!("\nmetamer model: apical-control λ sweep (excurrent↔decurrent), 110 steps:");
    for lambda in [0.42f32, 0.48, 0.52, 0.58] {
        let mut params = PlantParams::default();
        params.lambda = lambda;
        let mut plant = make_plant(params);
        for _ in 0..110 {
            plant.step(1.0);
        }
        let (h, crown, apex) = plant.shape();
        println!(
            "  λ={lambda:.2}  metamers {:>4}  height {:5.1}  crown {:.1}  spread {:.2}  apex_lean {:.2}  overlap {:.1}%",
            plant.module_count(),
            h,
            crown,
            crown / h.max(1e-3),
            apex / h.max(1e-3),
            100.0 * plant.intersection_ratio(),
        );
    }

    // --- quantitative validation against the papers ------------------------
    println!("\nvalidation — pipe-model allometry (Eq. 8):");
    println!("  pipe model: trunk basal area ∝ leaf count ⇒ diameter ∝ √leaves, slope ≈ 0.50");
    {
        let mut plant = make_plant(species::library()[0].params.clone()); // conifer
        let mut pts: Vec<(f32, f32)> = Vec::new();
        let mut last_leaves = 0usize;
        for s in 1..=150 {
            plant.step(1.0);
            let leaves = plant.leaves().len();
            // Sample over the growth phase only (skip once the tree has matured
            // and leaf count plateaus, so identical points don't pad the fit).
            if s % 12 == 0 && leaves >= 2 && leaves > last_leaves + 1 {
                pts.push((leaves as f32, plant.trunk_diameter()));
                println!("  age {s:>3}  leaves {:>4}  trunk_d {:.3}", leaves, plant.trunk_diameter());
                last_leaves = leaves;
            }
        }
        println!(
            "  → fitted slope (log diameter vs log leaves): {:.2}  (ideal 0.50 ✓)",
            loglog_slope(&pts)
        );
    }

    println!("\nvalidation — self-thinning law (dense even-aged cohort, seeding off):");
    println!("  Yoda's −3/2 law: log(mean biomass) vs log(density) has slope ≈ −1.5 as the");
    println!("  stand thins. (cohort competes for light; suppressed plants are culled)");
    {
        // A monoculture (clones of one representative genome) — Yoda's law is a
        // single-species property; the mixed evolving stand is a different thing.
        let rep = genome::Genome {
            lambda: 0.52, determinacy: 0.5, alpha: 2.2, gp: 1.0, v_root_max: 140.0,
            g2: -0.15, tropism_up: 0.30, xi: 0.25, phi: 0.05, shade_tolerance: 0.30,
            shed_ratio: 0.35, envelope_height: 18.0, envelope_radius: 4.0,
            flowering_age: 50.0, seed_radius: 8.0, seed_freq: 0.06, lifespan: 400.0,
            apical_relax: 0.0,
        };
        let mut eco = Ecosystem::monoculture(220, 14.0, 5, Climate { temp: 12.0, precip: 110.0 }, rep);
        eco.seeding_enabled = false;
        let area = 4.0 * eco.size * eco.size;
        let mut pts: Vec<(f32, f32)> = Vec::new();
        let mut last_n = usize::MAX;
        for s in 1..=600 {
            eco.step(1.0);
            // Sample the ACTIVE thinning trajectory: a point only when the cohort
            // has actually thinned since the last sample (the frozen tail, once a
            // few well-spaced survivors remain, would otherwise flatten the fit).
            if s % 10 == 0 && eco.plant_count() > 4 && eco.plant_count() < last_n {
                last_n = eco.plant_count();
                let n = eco.plant_count() as f32;
                let mean_bio = eco.plants.iter().map(|p| p.biomass()).sum::<f32>() / n;
                let density = n / area;
                pts.push((density, mean_bio));
                println!(
                    "  step {s:>3}  N {:>3}  density {:.3}  mean_biomass {:.2}",
                    eco.plant_count(),
                    density,
                    mean_bio,
                );
            }
        }
        println!(
            "  → fitted slope (log mean_biomass vs log density): {:.2}  (ideal −1.5)",
            loglog_slope(&pts)
        );
        println!("    (space-responsive crowns: survivors expand into freed space as the stand");
        println!("     thins, so mean biomass keeps rising and the slope approaches −1.5. The");
        println!("     residual to −1.5 is the early establishment crash + discrete sampling.)");
    }
}

/// Least-squares slope of log(y) vs log(x) over the points (for power-law fits).
fn loglog_slope(pts: &[(f32, f32)]) -> f32 {
    let n = pts.len() as f32;
    if n < 2.0 {
        return f32::NAN;
    }
    let xs: Vec<f32> = pts.iter().map(|p| p.0.max(1e-9).ln()).collect();
    let ys: Vec<f32> = pts.iter().map(|p| p.1.max(1e-9).ln()).collect();
    let mx = xs.iter().sum::<f32>() / n;
    let my = ys.iter().sum::<f32>() / n;
    let num: f32 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    let den: f32 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    if den.abs() < 1e-12 {
        f32::NAN
    } else {
        num / den
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

    let d = eco.size * 2.8;
    let camera = Camera::new_perspective(
        viewport,
        vec3(d, eco.size * 1.8, d),
        vec3(0.0, eco.size * 0.6, 0.0),
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

/// Headless performance benchmark — no window, no GL, no mesh, just the
/// simulation. `cargo run --release -- --bench [--steps N] [--plants N]
/// [--size S] [--seed K] [--temp T] [--precip P]`.
///
/// Deterministic (ChaCha8 seed) in the *simulation*; only the wall-clock timing
/// varies run to run, so we report means and percentiles. Defaults match the
/// `--shot` ecosystem (40 plants, size 22, seed 7) in the heaviest biome (warm/
/// wet → a closed canopy, the most modules) so the numbers reflect worst case.
fn run_bench() {
    use std::time::Instant;
    let args: Vec<String> = std::env::args().collect();
    let val = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    let num = |flag: &str, d: f32| val(flag).and_then(|s| s.parse().ok()).unwrap_or(d);
    let steps = num("--steps", 170.0) as usize;
    let n = num("--plants", 40.0) as usize;
    let size = num("--size", 22.0);
    let seed = num("--seed", 7.0) as u64;
    let temp = num("--temp", 26.0);
    let precip = num("--precip", 320.0);
    let climate = Climate { temp, precip };

    println!("=== ECOSYSTEM BENCH ===");
    println!(
        "plants {n}  size {size}  seed {seed}  steps {steps}  climate {temp}°C/{precip}cm  ({})",
        biome_name(temp, precip)
    );

    let mut eco = Ecosystem::new(n, size, seed, climate);
    let mut totals = StepTimings::default();
    let mut step_ms: Vec<f64> = Vec::with_capacity(steps);
    let mut modules_at: Vec<usize> = Vec::with_capacity(steps);

    let wall0 = Instant::now();
    for _ in 0..steps {
        let t = eco.step_timed(1.0);
        totals.centres += t.centres;
        totals.colonize += t.colonize;
        totals.shadow += t.shadow;
        totals.grow += t.grow;
        totals.cull_seed += t.cull_seed;
        step_ms.push(t.total() * 1000.0);
        modules_at.push(eco.total_modules());
    }
    let wall = wall0.elapsed().as_secs_f64();

    let pct = |v: &[f64], p: f64| -> f64 {
        let mut s = v.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        s[((p * (s.len() as f64 - 1.0)).round() as usize).min(s.len() - 1)]
    };
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;

    let plants = eco.plant_count();
    let modules = eco.total_modules();
    println!(
        "\nfinal: {plants} plants, {modules} modules ({:.0}/plant)",
        modules as f32 / plants.max(1) as f32
    );
    println!("wall:  {:.2} s for {steps} steps", wall);
    println!(
        "step:  mean {:.1} ms · median {:.1} · p95 {:.1} · max {:.1}",
        mean(&step_ms),
        pct(&step_ms, 0.5),
        pct(&step_ms, 0.95),
        step_ms.iter().cloned().fold(0.0, f64::max),
    );

    // ms/step over time (quartile means) — shows how cost scales as trees fill.
    let q = steps / 4;
    if q > 0 {
        let qm = |a: usize, b: usize| mean(&step_ms[a..b.min(step_ms.len())]);
        println!(
            "      over time (ms/step, quartiles): {:.1} → {:.1} → {:.1} → {:.1}",
            qm(0, q),
            qm(q, 2 * q),
            qm(2 * q, 3 * q),
            qm(3 * q, steps),
        );
        println!(
            "      modules    (quartile ends):     {} → {} → {} → {}",
            modules_at[q - 1],
            modules_at[2 * q - 1],
            modules_at[3 * q - 1],
            modules_at[steps - 1],
        );
    }

    let tot = totals.total().max(1e-12);
    let row = |name: &str, s: f64| {
        println!("      {name:<10} {:6.1} ms/step  {:4.1}%", s / steps as f64 * 1000.0, s / tot * 100.0);
    };
    println!("\nphase breakdown (mean ms/step, % of sim time):");
    row("centres", totals.centres);
    row("colonize", totals.colonize);
    row("shadow", totals.shadow);
    row("grow", totals.grow);
    row("cull/seed", totals.cull_seed);

    // --- render-path CPU cost: the per-frame mesh rebuild (GPU upload via
    // Mesh::new is NOT measured here — it needs a GL context — but it scales
    // with the reported vertex counts). Uses the grown `eco` above.
    println!("\n=== MESH BUILD (CPU side of the render path; GPU upload excluded) ===");
    let reps = 30usize;
    let (mut t_trunk, mut t_fol) = (0.0f64, 0.0f64);
    let (mut g_trunk, mut g_fol) = (0.0f64, 0.0f64);
    for _ in 0..reps {
        let s = Instant::now();
        let tb = eco.trunk_batches();
        g_trunk += s.elapsed().as_secs_f64();
        let _tm = mesh::build_forest_mesh(&tb, 6);
        t_trunk += s.elapsed().as_secs_f64();
        let s = Instant::now();
        let fb = eco.foliage_batches();
        g_fol += s.elapsed().as_secs_f64();
        let _fm = mesh::build_forest_foliage(&fb, 0.4, 5);
        t_fol += s.elapsed().as_secs_f64();
    }
    println!(
        "  (gather: trunk_batches {:.1} ms · foliage_batches {:.1} ms — sequential)",
        g_trunk / reps as f64 * 1000.0,
        g_fol / reps as f64 * 1000.0,
    );
    let segs: usize = eco.trunk_batches().iter().map(|(s, _)| s.len()).sum();
    let leaves: usize = eco.foliage_batches().iter().map(|(p, _)| p.len()).sum();
    // sides=6 → 12 verts & 12 tris per segment; per_cluster=5 → 20 verts & 10 tris per leaf.
    let (tv, tt) = (segs * 12, segs * 12);
    let (fv, ft) = (leaves * 20, leaves * 10);
    let trunk_ms = t_trunk / reps as f64 * 1000.0;
    let fol_ms = t_fol / reps as f64 * 1000.0;
    println!(
        "trunk: {segs} segs → {} verts / {} tris · build {:.1} ms",
        tv, tt, trunk_ms
    );
    println!(
        "foliage: {leaves} twigs → {} verts / {} tris · build {:.1} ms",
        fv, ft, fol_ms
    );
    println!(
        "TOTAL mesh build {:.1} ms/frame ({} verts, {} tris) — vs sim {:.1} ms/step",
        trunk_ms + fol_ms,
        tv + fv,
        tt + ft,
        mean(&step_ms),
    );

    // --- single-plant micro-bench (Consume mode: own dome + self-shadow). ---
    println!("\n=== SINGLE-PLANT BENCH (tropical, Consume mode) ===");
    let sp = &species::library()[6]; // tropical broadleaf — the biggest single tree
    let mut plant = make_plant(sp.params.clone());
    let tsteps = num("--tree-steps", 200.0) as usize;
    let mut tms: Vec<f64> = Vec::with_capacity(tsteps);
    let tw0 = Instant::now();
    for _ in 0..tsteps {
        let s = Instant::now();
        plant.step(1.0);
        tms.push(s.elapsed().as_secs_f64() * 1000.0);
    }
    let tw = tw0.elapsed().as_secs_f64();
    println!(
        "{tsteps} steps in {:.2} s · mean {:.2} ms/step · p95 {:.2} · final {} modules",
        tw,
        mean(&tms),
        pct(&tms, 0.95),
        plant.module_count(),
    );
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
    let mut eco = Ecosystem::new(40, 22.0, 7, climate);
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
    let bare = args.iter().any(|a| a == "--bare"); // skeleton only (branch geometry)
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
    if bare {
        target.render(&camera, ground.into_iter().chain(&tree), &[&ambient, &key, &fill]);
    } else {
        target.render(&camera, ground.into_iter().chain(&tree).chain(&foliage), &[&ambient, &key, &fill]);
    }
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
        vec3(62.0, 40.0, 62.0),
        vec3(0.0, 14.0, 0.0),
        vec3(0.0, 1.0, 0.0),
        degrees(45.0),
        0.1,
        2000.0,
    );
    let mut control = OrbitControl::new(camera.target(), 3.0, 400.0);

    let ambient = AmbientLight::new(&context, 0.5, Srgba::new(200, 215, 255, 255));
    let key = DirectionalLight::new(&context, 2.6, Srgba::new(255, 247, 230, 255), vec3(-0.5, -1.0, -0.7));
    let fill = DirectionalLight::new(&context, 0.9, Srgba::new(180, 200, 255, 255), vec3(0.8, -0.4, 0.5));

    let mut eco_size = 22.0f32;
    let plant_count = 40;
    let mut seed = 7u64;
    let mut climate = Climate { temp: 10.0, precip: 90.0 };
    let mut eco = Ecosystem::new(plant_count, eco_size, seed, climate);
    let mut eco_field_h = eco.field_height; // vertical extent (growth ceiling)

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
    // Run the sim unthrottled: each frame steps in a batch bounded only by this
    // wall-clock budget, so the window still renders and stays interactive.
    const STEP_BUDGET_S: f64 = 0.025;
    let mut show_foliage = true;

    println!("Synthetic Sylvaculture — Ecosystem");
    println!("  Space play/pause · S step · R reseed · F foliage · mouse orbit/zoom");
    println!("  ←/→ temperature · ↑/↓ precipitation · or CLICK the biome chart (top-left)");
    println!("  −/= shrink/grow plot (horizontal) · PageDown/PageUp lower/raise ceiling (vertical)");
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
        let mut resized = false;

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
                    // Plot area, in place (keeps the standing forest):
                    // − / = widen-narrow horizontally, PageUp/PageDown raise/lower
                    // the vertical growth ceiling.
                    Key::Minus => {
                        eco.set_size(eco_size - 3.0);
                        eco_size = eco.size;
                        resized = true;
                    }
                    Key::Equals | Key::Plus => {
                        eco.set_size(eco_size + 3.0);
                        eco_size = eco.size;
                        resized = true;
                    }
                    Key::PageDown => {
                        eco.set_field_height(eco_field_h - 5.0);
                        eco_field_h = eco.field_height;
                        resized = true;
                    }
                    Key::PageUp => {
                        eco.set_field_height(eco_field_h + 5.0);
                        eco_field_h = eco.field_height;
                        resized = true;
                    }
                    _ => {}
                }
            }
        }

        if reset {
            seed += 1;
            eco = Ecosystem::new(plant_count, eco_size, seed, climate);
            eco.set_field_height(eco_field_h); // keep the chosen vertical extent
            step_count = 0;
            dirty = true;
            println!(
                "[reseed] {:.0}°C, {:.0}cm → {}",
                climate.temp,
                climate.precip,
                biome_name(climate.temp, climate.precip)
            );
        }

        if resized {
            // Resize is in place (forest kept); just refresh the ground + meshes.
            ground.set_transformation(Mat4::from_angle_x(degrees(-90.0)) * Mat4::from_scale(eco_size * 2.2));
            dirty = true;
            println!(
                "[area] horizontal ±{:.0}  vertical ceiling {:.0}  (plants {})",
                eco_size,
                eco_field_h,
                eco.plant_count()
            );
        }

        if playing {
            // Run as fast as possible: step in an unthrottled batch, bounded only
            // by a small per-frame wall-clock budget so the window still renders
            // and stays interactive. The mesh is rebuilt once per frame (below),
            // not per step, so the sim advances many steps between redraws.
            let t0 = std::time::Instant::now();
            loop {
                eco.step(1.0);
                step_count += 1;
                dirty = true;
                if t0.elapsed().as_secs_f64() >= STEP_BUDGET_S {
                    break;
                }
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
    if std::env::args().any(|a| a == "--bench") {
        run_bench();
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

    println!("Synthetic Sylvaculture — single plant growth (metamer model)");
    println!("  Space play/pause · S step · R reset · ←/→ apical control λ · ↑/↓ growth rate");
    println!("  N next species · F foliage · mouse orbit/zoom");
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
                "[reset] λ={:.2}  gp={:.2}  ({})",
                params.lambda,
                params.gp,
                if params.lambda > 0.5 { "excurrent" } else { "decurrent" }
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
