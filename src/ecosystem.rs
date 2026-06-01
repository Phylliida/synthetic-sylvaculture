//! Ecosystem scale (Sec. 6): many plants growing together on a terrain.
//!
//! E1 (this stage): multiple plants of mixed species scattered on flat ground,
//! each an independent growth simulation, rendered as one combined mesh.
//! Global shadowing, seeding, and climate arrive in later stages.

use crate::plant::{colonize, pack, BudQuery, FxIdMap, FxMap, ModuleId, Occ, Plant, PlantParams, Segment};
use crate::genome::Genome;

/// Shared marker field: vertical extent and density (markers per unit volume).
const MAX_FIELD_HEIGHT: f32 = 34.0;
const FIELD_DENSITY: f32 = 0.6;
/// Space-colonization radii for the shared field (match PlantParams defaults).
const OCC_R: f32 = 1.1;
const PER_R: f32 = 2.8;
const PER_COS: f32 = 0.3;
/// Carbon-starvation mortality: a plant older than CARBON_ESTABLISH steps dies
/// once its smoothed crown-light (carbon income proxy) falls below the survival
/// bar `maintenance / P` (see `survival_bar`). Maintenance is an INTRINSIC cost
/// of crown size (wood respiration) plus a base — so a big crown only breaks
/// even where productivity P is high, regardless of crowding. This is what makes
/// climate select morphology robustly (not just under competition).
const CARBON_ESTABLISH: f32 = 30.0;
/// Baseline upkeep light (even a tiny plant must capture this × 1/P).
const MAINT_BASE: f32 = 0.06;
/// Extra upkeep a full-size crown demands (∝ crown volume) — the size cost.
const MAINT_SIZE: f32 = 0.22;
/// Crown volume that counts as "full size" for normalizing the size cost.
const MAINT_FULL_VOL: f32 = 2200.0;
/// Seed rain: establishment attempts scattered across the whole floor each step
/// (on top of local seeding), so gaps are colonized the moment they open and the
/// floor is always carpeted with seedlings trying to take hold. Bounded by the
/// plant cap, so a closed canopy leaves few openings and a sparse stand many.
const SEED_RAIN: usize = 10;
/// Fraction of seed-rain that is a fresh random genome (immigration): propagule
/// pressure + diversity, and lets a bare plot keep attempting to recolonize.
/// Small, so it doesn't dilute local adaptation (the misfits mostly die young).
const IMMIGRANT_FRAC: f32 = 0.1;
/// Plant-parallel grow: the per-plant growth cycles are independent (each reads
/// its own qg/space and the read-only shared centres/grid, mutates only itself),
/// so they run across this many contiguous plant chunks on scoped threads.
/// Bit-identical: each plant is processed in place, order preserved.
const GROW_CHUNKS: usize = 32;
use glam::{vec3, Vec3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::time::Instant;

/// Per-phase wall-clock breakdown of one `Ecosystem::step`, in seconds. The
/// benchmark harness (`--bench`) accumulates these to show where the step's
/// time goes (Instant overhead is ~ns, negligible against a ~100 ms step).
#[derive(Default, Clone, Copy)]
pub struct StepTimings {
    /// Per-plant `module_centres()` (reused by occupancy, shadow, and g lookup).
    pub centres: f64,
    /// Bud + wood gather, the shared-field `colonize`, and the `space` scatter.
    pub colonize: f64,
    /// Global shadow grid allocation + deposition.
    pub shadow: f64,
    /// Per-plant `step_in_field` (the metamer growth cycle).
    pub grow: f64,
    /// Mortality cull + seeding.
    pub cull_seed: f64,
}

impl StepTimings {
    pub fn total(&self) -> f64 {
        self.centres + self.colonize + self.shadow + self.grow + self.cull_seed
    }
}

/// Global shadow-propagation grid (Pałubicki 2009; Silviculture Sec. 6.2).
/// Each module casts a downward pyramidal penumbra into a voxel grid; a module
/// then reads its light availability Q_G = max(C − s + a, 0)/C from the grid.
/// The `+a` cancels a module's self-shadow.
struct ShadowGrid {
    min: Vec3,
    cell: f32,
    nx: usize,
    ny: usize,
    nz: usize,
    s: Vec<f32>,
    a: f32,
    b: f32,
    c: f32,
    qmax: i32,
}

impl ShadowGrid {
    fn new(size: f32, max_y: f32, cell: f32) -> Self {
        let margin = 3.0;
        let span = (size + margin) * 2.0;
        let nx = (span / cell).ceil() as usize + 1;
        let ny = (max_y / cell).ceil() as usize + 1;
        let nz = nx;
        ShadowGrid {
            min: vec3(-size - margin, 0.0, -size - margin),
            cell,
            nx,
            ny,
            nz,
            s: vec![0.0; nx * ny * nz],
            a: 1.0,
            b: 2.0,
            c: 8.0,
            qmax: 6,
        }
    }

    fn ijk(&self, p: Vec3) -> (i32, i32, i32) {
        (
            ((p.x - self.min.x) / self.cell).floor() as i32,
            ((p.y - self.min.y) / self.cell).floor() as i32,
            ((p.z - self.min.z) / self.cell).floor() as i32,
        )
    }

    fn idx(&self, i: i32, j: i32, k: i32) -> usize {
        i as usize + self.nx * (k as usize + self.nz * j as usize)
    }

    /// Bin module centres to integer cells, then deposit one weighted pyramid
    /// per occupied cell. Many modules share a voxel, and N identical pyramids
    /// sum to one pyramid weighted by N (addition commutes), so this is exactly
    /// equivalent to depositing per module — but at canopy density it is far
    /// fewer pyramids than modules, so much cheaper.
    fn deposit_binned(&mut self, centres: &[Vec3]) {
        // cell key -> (ci, cj, ck, count)
        let mut cells: FxMap<(i32, i32, i32, u32)> = FxMap::default();
        for &p in centres {
            let (ci, cj, ck) = self.ijk(p);
            let e = cells.entry(pack((ci, cj, ck))).or_insert((ci, cj, ck, 0));
            e.3 += 1;
        }
        let (nx, ny, nz) = (self.nx as i32, self.ny as i32, self.nz as i32);
        for &(ci, cj, ck, n) in cells.values() {
            let w = n as f32;
            for q in 0..=self.qmax {
                let j = cj - q; // shadow propagates downward
                if j < 0 {
                    break;
                }
                if j >= ny {
                    continue; // cell above the grid top (rare) — this layer only
                }
                let ds = self.a * self.b.powi(-q) * w;
                // Clamp the (di, dk) block to the grid once, so the inner loop is
                // branch-free and indexes a contiguous row (one multiply / row).
                let (i0, i1) = ((ci - q).max(0), (ci + q).min(nx - 1));
                let (k0, k1) = ((ck - q).max(0), (ck + q).min(nz - 1));
                for k in k0..=k1 {
                    let row = self.nx * (k as usize + self.nz * j as usize);
                    for i in i0..=i1 {
                        self.s[row + i as usize] += ds;
                    }
                }
            }
        }
    }

    fn light_at(&self, p: Vec3) -> f32 {
        let (i, j, k) = self.ijk(p);
        let i = i.clamp(0, self.nx as i32 - 1);
        let j = j.clamp(0, self.ny as i32 - 1);
        let k = k.clamp(0, self.nz as i32 - 1);
        let sv = self.s[self.idx(i, j, k)];
        ((self.c - sv + self.a).max(0.0) / self.c).clamp(0.0, 1.0)
    }
}

pub struct Ecosystem {
    pub plants: Vec<Plant>,
    /// Genome of each plant (parallel to `plants`). The ecosystem evolves: there
    /// is no fixed species list — founders get random genomes and seeds inherit
    /// the parent's genome with mutation, so morphology is sculpted by selection.
    pub genomes: Vec<Genome>,
    /// Half-extent of the square ground.
    pub size: f32,
    pub age: f32,
    /// Global shadowing on/off (Sec. 6.2). Off = plants ignore each other's shade.
    pub shadow_enabled: bool,
    /// Seeding/recruitment on/off. Off = a fixed even-aged cohort (used by the
    /// self-thinning validation, where new recruits would muddle the law).
    pub seeding_enabled: bool,
    /// Climate (Sec. 6.4). Couples to growth ONLY via a single physical
    /// productivity scalar `P` (`productivity()`) — no per-genome niche. Biome
    /// specialization is emergent: different morphologies win at different `P`.
    pub climate: Climate,
    /// Per-trait mutation step (fraction of each trait's span) for seeds.
    pub mutation_rate: f32,
    /// Population cap (for interactive performance).
    pub max_plants: usize,
    /// Shared free-space marker field for space colonization (Pałubicki §4.1):
    /// all plants' buds compete for these, so crowding genuinely shapes tree
    /// size and survivors expand into the space freed when neighbours die.
    /// Persistent (occupancy is recomputed vs current wood, not deleted).
    markers: Vec<Vec3>,
    rng: ChaCha8Rng,
}

/// Average annual temperature (°C) and precipitation (cm) for the environment.
#[derive(Clone, Copy, Debug)]
pub struct Climate {
    pub temp: f32,
    pub precip: f32,
}

impl Climate {
    /// Physical productivity `P ∈ [0,1]`: how much carbon this environment can
    /// make. Rises monotonically and saturates with both warmth and water (this
    /// is the Whittaker net-primary-productivity gradient). NOT a per-genome
    /// niche and NOT peaked at any optimum — every plant feels the same `P`. It
    /// is the ONLY way climate touches the simulation: it scales growth
    /// (`v_root_max *= P`) and the carbon-starvation survival bar
    /// (`health < THRESHOLD / P`). Specialization to a biome therefore emerges
    /// purely from which morphologies break even / out-compete at this `P`.
    pub fn productivity(&self) -> f32 {
        // Logistic in each axis: ~0 when cold/dry, ~1 when warm/wet.
        let warmth = 1.0 / (1.0 + (-(self.temp - 6.0) / 5.0).exp());
        let water = 1.0 / (1.0 + (-(self.precip - 65.0) / 28.0).exp());
        (warmth * water).clamp(0.0, 1.0)
    }
}

/// Coarse Whittaker-style biome label for a climate point (Fig. 2).
pub fn biome_name(t: f32, p: f32) -> &'static str {
    if t < 0.0 {
        "tundra"
    } else if t < 7.0 {
        if p < 40.0 { "cold desert / grassland" } else { "boreal forest" }
    } else if t < 20.0 {
        if p < 40.0 {
            "temperate grassland"
        } else if p < 100.0 {
            "temperate seasonal forest"
        } else {
            "temperate rainforest"
        }
    } else if p < 50.0 {
        "subtropical desert"
    } else if p < 150.0 {
        "savanna / tropical seasonal"
    } else {
        "tropical rainforest"
    }
}

impl Ecosystem {
    pub fn new(n: usize, size: f32, seed: u64, climate: Climate) -> Self {
        let rng = ChaCha8Rng::seed_from_u64(seed);
        let mut eco = Ecosystem {
            plants: Vec::new(),
            genomes: Vec::new(),
            size,
            age: 0.0,
            shadow_enabled: true,
            seeding_enabled: true,
            climate,
            mutation_rate: 0.08,
            max_plants: 170,
            markers: Vec::new(),
            rng,
        };

        // Shared free-space field over the plot (a box up to MAX_FIELD_HEIGHT).
        // Density is modest for performance; the stand competes for these points.
        let max_h = MAX_FIELD_HEIGHT;
        let count = (FIELD_DENSITY * (2.0 * size) * (2.0 * size) * max_h) as usize;
        for _ in 0..count {
            let x = eco.rng.gen_range(-size..size);
            let z = eco.rng.gen_range(-size..size);
            let y = eco.rng.gen_range(0.0..max_h);
            eco.markers.push(vec3(x, y, z));
        }

        // Founders: a uniform-random genome each (a broad initial gene pool).
        // Specialization to the climate emerges from selection on these, not
        // from any climate-aware seeding.
        for _ in 0..n {
            let x = eco.rng.gen_range(-size..size);
            let z = eco.rng.gen_range(-size..size);
            let g = Genome::random(&mut eco.rng);
            let plant = eco.make_plant_from_genome(&g, vec3(x, 0.0, z));
            eco.plants.push(plant);
            eco.genomes.push(g);
        }
        eco
    }

    /// A monospecific even-aged cohort: `n` clones of one genome, scattered on
    /// the plot. Used for the self-thinning (Yoda −3/2) validation, which is a
    /// property of a single species competing with itself — not the mixed,
    /// evolving stand `new` builds.
    pub fn monoculture(n: usize, size: f32, seed: u64, climate: Climate, g: Genome) -> Self {
        let mut eco = Ecosystem::new(0, size, seed, climate);
        for _ in 0..n {
            let x = eco.rng.gen_range(-size..size);
            let z = eco.rng.gen_range(-size..size);
            let plant = eco.make_plant_from_genome(&g, vec3(x, 0.0, z));
            eco.plants.push(plant);
            eco.genomes.push(g.clone());
        }
        eco
    }

    /// Build the plant a genome expresses at `pos`. Climate enters here as the
    /// single physical productivity scalar: `v_root_max *= P`, so the *same*
    /// genome grows large in a rich climate and stays small in a poor one (no
    /// per-genome niche). Its private marker dome is cleared — in the ecosystem
    /// it grows in the shared field instead.
    fn make_plant_from_genome(&self, g: &Genome, pos: Vec3) -> Plant {
        let p = self.climate.productivity();
        let mut params = g.to_params();
        params.v_root_max *= p.max(0.12); // growth budget scales with productivity
        let mut plant = Plant::new(params, pos);
        plant.clear_markers();
        plant
    }

    pub fn step(&mut self, dt: f32) {
        let _ = self.step_timed(dt);
    }

    /// Like `step`, but returns a per-phase wall-clock breakdown (`--bench`).
    pub fn step_timed(&mut self, dt: f32) -> StepTimings {
        let mut t = StepTimings::default();
        self.age += dt;

        // --- 1. shared space colonization: all plants' buds compete for the one
        // marker field. Occupancy is recomputed vs current wood (so dead trees
        // free their space); each free marker goes to the nearest perceiving bud.
        // Per-plant internode centres, computed once and reused (wood occupancy,
        // shadow deposition, and the g lookup).
        let c0 = Instant::now();
        let centres: Vec<Vec<(ModuleId, Vec3)>> =
            self.plants.iter().map(|p| p.module_centres()).collect();
        t.centres = c0.elapsed().as_secs_f64();

        let k0 = Instant::now();
        let mut bud_keys: Vec<(usize, ModuleId)> = Vec::new();
        let mut buds: Vec<BudQuery> = Vec::new();
        let mut wood: Vec<Vec3> = Vec::new();
        for (pi, p) in self.plants.iter().enumerate() {
            let ceiling = p.reveal_ceiling();
            let crown_r = p.params.envelope_radius + p.params.internode_len;
            for (id, pos, dir) in p.active_buds() {
                bud_keys.push((pi, id));
                buds.push(BudQuery { pos, dir, ceiling, center: p.origin, crown_r });
            }
            wood.extend(centres[pi].iter().map(|(_, c)| *c));
        }
        // Occupancy is only ever queried at marker cells, and all markers live
        // in this fixed field box; wood outside it lands in cells no marker
        // occupies, so bounding the dense grid to the box is exactly equivalent
        // and lets it build in one pass (no bbox scan).
        let bounds = (
            vec3(-self.size, 0.0, -self.size),
            vec3(self.size, MAX_FIELD_HEIGHT, self.size),
        );
        let vs = colonize(
            &mut self.markers,
            Occ::Wood(&wood),
            &buds,
            OCC_R,
            PER_R,
            PER_COS,
            Some(bounds),
        );
        let mut space: Vec<FxIdMap<Vec3>> = vec![FxIdMap::default(); self.plants.len()];
        for (i, v) in vs.into_iter().enumerate() {
            if let Some(dir) = v {
                let (pi, id) = bud_keys[i];
                space[pi].insert(id, dir);
            }
        }
        t.colonize = k0.elapsed().as_secs_f64();

        // --- 2. global shadow grid → per-module light g (inter-plant shading).
        let s0 = Instant::now();
        let grid = if self.shadow_enabled {
            let mut g = ShadowGrid::new(self.size, MAX_FIELD_HEIGHT, 1.5);
            // `wood` is the same flattened set of module centres (built above for
            // occupancy), still in scope — reuse it rather than reflatten.
            g.deposit_binned(&wood);
            Some(g)
        } else {
            None
        };
        t.shadow = s0.elapsed().as_secs_f64();

        // --- 3. grow each plant in the shared field. Plants are independent, so
        // run them across scoped threads (each builds its own qg from the shared
        // read-only centres/grid). Contiguous &mut chunks, processed in place →
        // bit-identical to the sequential loop.
        let g0 = Instant::now();
        {
            let grid_ref = grid.as_ref();
            let centres_ref = &centres;
            let space_ref = &space;
            let nplants = self.plants.len();
            let n_chunks = GROW_CHUNKS
                .min(std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
                .max(1);
            let chunk_size = nplants.div_ceil(n_chunks).max(1);
            std::thread::scope(|scope| {
                let mut base = 0usize;
                for chunk in self.plants.chunks_mut(chunk_size) {
                    let start = base;
                    base += chunk.len();
                    scope.spawn(move || {
                        for (k, p) in chunk.iter_mut().enumerate() {
                            let pi = start + k;
                            let qg: FxIdMap<f32> = centres_ref[pi]
                                .iter()
                                .map(|(id, c)| {
                                    (*id, grid_ref.map(|g| g.light_at(*c)).unwrap_or(1.0))
                                })
                                .collect();
                            p.step_in_field(dt, &qg, &space_ref[pi]);
                        }
                    });
                }
            });
        }
        t.grow = g0.elapsed().as_secs_f64();

        let m0 = Instant::now();
        self.cull_dead();
        if self.seeding_enabled {
            self.seed(dt);
        }
        t.cull_seed = m0.elapsed().as_secs_f64();
        t
    }

    /// The minimum smoothed crown-light a plant of these traits must hold to pay
    /// its upkeep here. Income is `P · health`; maintenance = base + a size cost
    /// (∝ crown volume, an intrinsic wood-respiration cost), reduced by shade
    /// tolerance (cheap, durable leaves). Survive iff `P · health ≥ maintenance`,
    /// i.e. `health ≥ maintenance / P`. Because the size cost is intrinsic (not
    /// competition-driven), a big crown only breaks even at high productivity —
    /// so climate selects size robustly, even in a sparse stand.
    fn survival_bar(&self, params: &PlantParams) -> f32 {
        let p = self.climate.productivity().max(0.05);
        let vol = std::f32::consts::PI
            * params.envelope_radius
            * params.envelope_radius
            * params.envelope_height;
        let size = (vol / MAINT_FULL_VOL).clamp(0.0, 1.1);
        let maint = (MAINT_BASE + MAINT_SIZE * size) * (1.0 - 0.5 * params.shade_tolerance);
        (maint / p).clamp(0.02, 0.97)
    }

    /// Remove plants that die, opening gaps (Sec. 4.2): old age (senescence), or
    /// **carbon starvation** — an established plant whose smoothed crown-light has
    /// fallen below its `survival_bar` (too small a crown to pay upkeep at this
    /// productivity, or overtopped). Death opens space and sharpens succession.
    fn cull_dead(&mut self) {
        // Carbon balance with climate: survive only if income (light) covers
        // upkeep, i.e. `health · P ≥ cost`, so the survival light bar is
        // `CARBON_THRESHOLD / P`. Rich climate → low bar → tall dense canopy
        // viable; poor climate → high bar → only small, well-lit forms break
        // even (sparse scrub). This — not any niche — makes climate select
        // morphology.
        let dead: Vec<bool> = self
            .plants
            .iter()
            .map(|pl| {
                let bar = self.survival_bar(&pl.params);
                let senesced = pl.age >= 1.9 * pl.params.p_max;
                let starved = pl.age > CARBON_ESTABLISH && pl.health() < bar;
                senesced || starved
            })
            .collect();
        let mut i = 0;
        self.plants.retain(|_| {
            let keep = !dead[i];
            i += 1;
            keep
        });
        let mut j = 0;
        self.genomes.retain(|_| {
            let keep = !dead[j];
            j += 1;
            keep
        });
    }

    /// Mature, thriving plants scatter seeds nearby (Sec. 6.3); each seed
    /// **inherits the parent genome with a small mutation** (heritable variation
    /// — the ingredient that lets selection accumulate). There is no climate
    /// niche: reproduction just needs a plant past its (genome-set) flowering age
    /// that is paying its upkeep (`health ≥ CARBON_THRESHOLD`), so only forms
    /// that actually thrive in this climate spread their genes.
    fn seed(&mut self, dt: f32) {
        if self.plants.len() >= self.max_plants {
            return;
        }
        // Snapshot the parents first (releases the &self borrow before rng use).
        // `bar` is each parent's own survival bar — only a plant comfortably
        // paying its upkeep (health above bar) flowers, so reproductive success
        // tracks how well the genome actually fits this climate.
        let parents: Vec<(f32, f32, f32, Genome, Vec3)> = self
            .plants
            .iter()
            .zip(&self.genomes)
            .map(|(pl, g)| (pl.age, pl.health(), self.survival_bar(&pl.params), g.clone(), pl.origin))
            .collect();
        let mut newborns: Vec<(Genome, Vec3)> = Vec::new();
        for (age, health, bar, g, origin) in &parents {
            if self.plants.len() + newborns.len() >= self.max_plants {
                break;
            }
            if *age < g.flowering_age || *health < *bar {
                continue;
            }
            if self.rng.gen::<f32>() < g.seed_freq * dt {
                let ang = self.rng.gen::<f32>() * std::f32::consts::TAU;
                let r = g.seed_radius * self.rng.gen::<f32>().sqrt();
                let x = (origin.x + ang.cos() * r).clamp(-self.size, self.size);
                let z = (origin.z + ang.sin() * r).clamp(-self.size, self.size);
                let child = g.mutated(self.mutation_rate, &mut self.rng);
                newborns.push((child, vec3(x, 0.0, z)));
            }
        }
        for (g, pos) in newborns {
            let plant = self.make_plant_from_genome(&g, pos);
            self.plants.push(plant);
            self.genomes.push(g);
        }

        // Seed rain: keep the floor carpeted with establishment attempts so a gap
        // is colonized the moment a plant dies. Rain genomes come from the proven
        // reproductive pool (mature, thriving plants) + a small fresh-random
        // immigrant fraction. Most land in shade and starve young; the ones in
        // gaps take hold — recruitment by competition, not a schedule.
        let pool: Vec<Genome> = parents
            .iter()
            .filter(|(age, health, bar, g, _)| *age >= g.flowering_age && *health >= *bar)
            .map(|(_, _, _, g, _)| g.clone())
            .collect();
        let mut rained = 0;
        while self.plants.len() < self.max_plants && rained < SEED_RAIN {
            rained += 1;
            let g = if !pool.is_empty() && self.rng.gen::<f32>() > IMMIGRANT_FRAC {
                let i = self.rng.gen_range(0..pool.len());
                pool[i].mutated(self.mutation_rate, &mut self.rng)
            } else {
                Genome::random(&mut self.rng)
            };
            let x = self.rng.gen_range(-self.size..self.size);
            let z = self.rng.gen_range(-self.size..self.size);
            let plant = self.make_plant_from_genome(&g, vec3(x, 0.0, z));
            self.plants.push(plant);
            self.genomes.push(g);
        }
    }

    /// Number of *established* plants — those past the seedling gauntlet
    /// (`age > CARBON_ESTABLISH`). The seed rain keeps the floor full of young
    /// seedlings, so the total `plant_count` is mostly transient carpet; the
    /// established count is the standing adapted community.
    pub fn established_count(&self) -> usize {
        self.plants.iter().filter(|p| p.age > CARBON_ESTABLISH).count()
    }

    /// Mean of each genome trait over the **established** plants (field order;
    /// see `Genome::NAMES`). Established only — averaging the seedling carpet
    /// (≈ the seed-rain source) would mask what selection actually favoured.
    /// `None` when nothing has established (e.g. a climate too harsh to survive).
    pub fn mean_traits(&self) -> Option<[f32; 17]> {
        let est: Vec<&Genome> = self
            .plants
            .iter()
            .zip(&self.genomes)
            .filter(|(p, _)| p.age > CARBON_ESTABLISH)
            .map(|(_, g)| g)
            .collect();
        if est.is_empty() {
            return None;
        }
        let mut acc = [0.0f32; 17];
        for g in &est {
            let t = g.traits();
            for k in 0..17 {
                acc[k] += t[k];
            }
        }
        let n = est.len() as f32;
        for v in &mut acc {
            *v /= n;
        }
        Some(acc)
    }

    pub fn plant_count(&self) -> usize {
        self.plants.len()
    }

    pub fn total_modules(&self) -> usize {
        self.plants.iter().map(|p| p.module_count()).sum()
    }

    pub fn plant_heights(&self) -> Vec<f32> {
        self.plants.iter().map(|p| p.height()).collect()
    }

    /// Contiguous plant-index chunks for parallel per-plant gather, in order.
    fn plant_chunks(&self) -> Vec<(usize, usize)> {
        let n = self.plants.len();
        if n == 0 {
            return Vec::new();
        }
        let nc = GROW_CHUNKS
            .min(std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
            .min(n)
            .max(1);
        let cs = n.div_ceil(nc).max(1);
        (0..n).step_by(cs).map(|s| (s, (s + cs).min(n))).collect()
    }

    /// Per-plant trunk segments tinted with that plant's bark colour. The
    /// per-plant `skeleton()` calls are independent, so gather them in parallel
    /// (order preserved: chunks are contiguous and flattened in order).
    pub fn trunk_batches(&self) -> Vec<(Vec<Segment>, [u8; 3])> {
        let (plants, genomes) = (&self.plants, &self.genomes);
        let parts: Vec<Vec<(Vec<Segment>, [u8; 3])>> = std::thread::scope(|scope| {
            let handles: Vec<_> = self
                .plant_chunks()
                .into_iter()
                .map(|(s, e)| {
                    scope.spawn(move || {
                        (s..e)
                            .map(|i| (plants[i].skeleton(), genomes[i].bark_rgb()))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        parts.into_iter().flatten().collect()
    }

    /// Per-plant leaf points tinted with that plant's leaf colour (parallel
    /// gather like `trunk_batches`).
    pub fn foliage_batches(&self) -> Vec<(Vec<(Vec3, Vec3)>, [u8; 3])> {
        let (plants, genomes) = (&self.plants, &self.genomes);
        let parts: Vec<Vec<(Vec<(Vec3, Vec3)>, [u8; 3])>> = std::thread::scope(|scope| {
            let handles: Vec<_> = self
                .plant_chunks()
                .into_iter()
                .map(|(s, e)| {
                    scope.spawn(move || {
                        (s..e)
                            .map(|i| (plants[i].leaves(), genomes[i].leaf_rgb()))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        parts.into_iter().flatten().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grown(climate: Climate, steps: u32) -> Ecosystem {
        let mut eco = Ecosystem::new(30, 13.0, 9, climate);
        for _ in 0..steps {
            eco.step(1.0);
        }
        eco
    }

    #[test]
    fn seeding_grows_and_bounds_population() {
        let eco = grown(Climate { temp: 10.0, precip: 90.0 }, 220);
        // Flowering plants must have seeded new ones...
        assert!(eco.plant_count() > 30, "no recruitment: {}", eco.plant_count());
        // ...but never beyond the population cap.
        assert!(eco.plant_count() <= eco.max_plants);
    }

    #[test]
    fn climate_selects_taller_crowns_where_productive() {
        // The headline emergent property: there is NO hardcoded climate niche —
        // a productive (warm, wet) climate must evolve a taller mean crown than a
        // poor (cold, dry) one. Specialization is pure selection on the carbon
        // tradeoff (crown light self-shades, so a big crown only pays its way at
        // high productivity). The established cohort is small ⇒ per-run noisy, so
        // average the evolved mean crown height (env_h) over several seeds.
        let mean_env_h = |climate: Climate| -> f32 {
            let seeds = [1u64, 2, 3, 4];
            let hs: Vec<f32> = seeds
                .iter()
                .filter_map(|&sd| {
                    let mut eco = Ecosystem::new(36, 13.0, sd, climate);
                    for _ in 0..340 {
                        eco.step(1.0);
                    }
                    eco.mean_traits().map(|m| m[11]) // env_h
                })
                .collect();
            hs.iter().sum::<f32>() / hs.len().max(1) as f32
        };
        let poor = mean_env_h(Climate { temp: 5.0, precip: 80.0 });
        let rich = mean_env_h(Climate { temp: 24.0, precip: 280.0 });
        assert!(
            rich > poor + 2.0,
            "productive climate should evolve taller crowns (avg over seeds): rich env_h {rich:.1} vs poor {poor:.1}"
        );
    }

    #[test]
    fn forest_canopy_stays_upright() {
        // Regression for the banana/loop bug: in a grown stand the tall plants
        // must rise roughly over their bases, not arc over. apex_lean is the
        // highest node's horizontal offset / height. (Productive climate so the
        // stand actually grows tall plants to check.)
        let mut eco = Ecosystem::new(40, 14.0, 7, Climate { temp: 22.0, precip: 220.0 });
        for _ in 0..160 {
            eco.step(1.0);
        }
        let mut leans: Vec<f32> = eco
            .plants
            .iter()
            .filter_map(|p| {
                let (h, _, apex) = p.shape();
                (h > 6.0).then_some(apex / h)
            })
            .collect();
        assert!(!leans.is_empty(), "expected some tall plants");
        // Median (robust to the few gnarly high-ξ / low-tropism genomes the
        // evolving population naturally contains): the banana/loop bug arced
        // *every* tall plant right over (lean ≫ 0.5); a healthy stand sits low.
        leans.sort_by(f32::total_cmp);
        let median = leans[leans.len() / 2];
        assert!(median < 0.4, "forest canopy is arcing over: median apex_lean {median:.2}");
    }

    #[test]
    fn shadowing_suppresses_total_biomass() {
        let climate = Climate { temp: 10.0, precip: 90.0 };
        let mut lit = Ecosystem::new(40, 13.0, 7, climate);
        lit.shadow_enabled = false;
        let mut shaded = Ecosystem::new(40, 13.0, 7, climate);
        shaded.shadow_enabled = true;
        for _ in 0..140 {
            lit.step(1.0);
            shaded.step(1.0);
        }
        assert!(
            shaded.total_modules() < lit.total_modules(),
            "shadowing should suppress biomass: shaded {} vs lit {}",
            shaded.total_modules(),
            lit.total_modules()
        );
    }
}
