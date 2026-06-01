//! Ecosystem scale (Sec. 6): many plants growing together on a terrain.
//!
//! E1 (this stage): multiple plants of mixed species scattered on flat ground,
//! each an independent growth simulation, rendered as one combined mesh.
//! Global shadowing, seeding, and climate arrive in later stages.

use crate::plant::{colonize, pack, BudQuery, FxIdMap, FxMap, ModuleId, Occ, Plant, Segment};
use crate::species::{self, Species};

/// Shared marker field: vertical extent and density (markers per unit volume).
const MAX_FIELD_HEIGHT: f32 = 34.0;
const FIELD_DENSITY: f32 = 0.6;
/// Space-colonization radii for the shared field (match PlantParams defaults).
const OCC_R: f32 = 1.1;
const PER_R: f32 = 2.8;
const PER_COS: f32 = 0.3;
/// Carbon-starvation mortality: a plant older than CARBON_ESTABLISH steps dies
/// once its smoothed carbon balance (mean foliage light) falls below
/// CARBON_THRESHOLD — too shaded to pay its upkeep. Shade tolerance floors a
/// species' light, so tolerant climax species survive shade that kills pioneers.
const CARBON_ESTABLISH: f32 = 30.0;
const CARBON_THRESHOLD: f32 = 0.18;
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
    pub species: Vec<Species>,
    pub plants: Vec<Plant>,
    /// Species index of each plant (parallel to `plants`).
    pub species_idx: Vec<usize>,
    /// Half-extent of the square ground.
    pub size: f32,
    pub age: f32,
    /// Global shadowing on/off (Sec. 6.2). Off = plants ignore each other's shade.
    pub shadow_enabled: bool,
    /// Seeding/recruitment on/off. Off = a fixed even-aged cohort (used by the
    /// self-thinning validation, where new recruits would muddle the law).
    pub seeding_enabled: bool,
    /// Climate (Sec. 6.4): drives per-species adaptation o (Eq. 11).
    pub climate: Climate,
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
        let species = species::library();
        let rng = ChaCha8Rng::seed_from_u64(seed);
        let mut eco = Ecosystem {
            species,
            plants: Vec::new(),
            species_idx: Vec::new(),
            size,
            age: 0.0,
            shadow_enabled: true,
            seeding_enabled: true,
            climate,
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

        // Initial even-aged cohort (age variety later emerges from seeding).
        for _ in 0..n {
            let x = eco.rng.gen_range(-size..size);
            let z = eco.rng.gen_range(-size..size);
            let si = eco.pick_species_for_climate();
            let plant = eco.make_plant_of(si, vec3(x, 0.0, z));
            eco.plants.push(plant);
            eco.species_idx.push(si);
        }
        eco
    }

    /// Build a plant of species `si` at `pos`, with its growth potential scaled
    /// by climate adaptation o (Eq. 11) — poorly-adapted species barely grow.
    /// Its private marker dome is cleared: in the ecosystem it grows in the
    /// shared field instead.
    fn make_plant_of(&self, si: usize, pos: Vec3) -> Plant {
        let sp = &self.species[si];
        let o = sp.adaptation(self.climate.temp, self.climate.precip);
        let mut params = sp.params.clone();
        params.v_root_max *= o; // total growth potential scales with adaptation
        let mut plant = Plant::new(params, pos);
        plant.clear_markers();
        plant
    }

    /// Pick a species with probability proportional to its climate adaptation,
    /// so a stand starts dominated by species suited to the environment.
    fn pick_species_for_climate(&mut self) -> usize {
        let weights: Vec<f32> = self
            .species
            .iter()
            .map(|s| s.adaptation(self.climate.temp, self.climate.precip) + 0.01)
            .collect();
        let total: f32 = weights.iter().sum();
        let mut r = self.rng.gen::<f32>() * total;
        for (i, w) in weights.iter().enumerate() {
            r -= w;
            if r <= 0.0 {
                return i;
            }
        }
        weights.len() - 1
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

    /// Remove plants that die, opening gaps (Sec. 4.2): old age (senescence), or
    /// **carbon starvation** — an established plant whose smoothed carbon balance
    /// (resource captured per metamer) has fallen below what it needs to pay its
    /// upkeep. The latter is competition-driven death: overtopped, shaded trees
    /// can no longer sustain their wood and die, sharpening succession.
    fn cull_dead(&mut self) {
        let dead: Vec<bool> = self
            .plants
            .iter()
            .map(|p| {
                let senesced = p.age >= 1.9 * p.params.p_max;
                let starved = p.age > CARBON_ESTABLISH && p.health() < CARBON_THRESHOLD;
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
        self.species_idx.retain(|_| {
            let keep = !dead[j];
            j += 1;
            keep
        });
    }

    /// Flowering plants scatter seeds of their own species nearby (Sec. 6.3);
    /// seeding rate scales with climate adaptation, and offspring inherit the
    /// climate-scaled growth potential, so well-adapted species spread.
    fn seed(&mut self, dt: f32) {
        if self.plants.len() >= self.max_plants {
            return;
        }
        let (t, p) = (self.climate.temp, self.climate.precip);
        let mut newborns: Vec<(usize, Vec3)> = Vec::new();
        for (plant, &si) in self.plants.iter().zip(&self.species_idx) {
            if self.plants.len() + newborns.len() >= self.max_plants {
                break;
            }
            let sp = &self.species[si];
            if plant.age < sp.flowering_age {
                continue;
            }
            let o = sp.adaptation(t, p);
            if self.rng.gen::<f32>() < sp.seed_freq * o * dt {
                let ang = self.rng.gen::<f32>() * std::f32::consts::TAU;
                let r = sp.seed_radius * self.rng.gen::<f32>().sqrt();
                let x = (plant.origin.x + ang.cos() * r).clamp(-self.size, self.size);
                let z = (plant.origin.z + ang.sin() * r).clamp(-self.size, self.size);
                newborns.push((si, vec3(x, 0.0, z)));
            }
        }
        for (si, pos) in newborns {
            let plant = self.make_plant_of(si, pos);
            self.plants.push(plant);
            self.species_idx.push(si);
        }
    }

    /// Counts of each species currently present (parallel to the library).
    pub fn species_counts(&self) -> Vec<usize> {
        let mut counts = vec![0; self.species.len()];
        for &si in &self.species_idx {
            counts[si] += 1;
        }
        counts
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
        let (plants, sidx, species) = (&self.plants, &self.species_idx, &self.species);
        let parts: Vec<Vec<(Vec<Segment>, [u8; 3])>> = std::thread::scope(|scope| {
            let handles: Vec<_> = self
                .plant_chunks()
                .into_iter()
                .map(|(s, e)| {
                    scope.spawn(move || {
                        (s..e)
                            .map(|i| {
                                let c = species[sidx[i]].bark_rgb;
                                (plants[i].skeleton(), [c.0, c.1, c.2])
                            })
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
        let (plants, sidx, species) = (&self.plants, &self.species_idx, &self.species);
        let parts: Vec<Vec<(Vec<(Vec3, Vec3)>, [u8; 3])>> = std::thread::scope(|scope| {
            let handles: Vec<_> = self
                .plant_chunks()
                .into_iter()
                .map(|(s, e)| {
                    scope.spawn(move || {
                        (s..e)
                            .map(|i| {
                                let c = species[sidx[i]].leaf_rgb;
                                (plants[i].leaves(), [c.0, c.1, c.2])
                            })
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
    fn warm_climate_favors_thermophile() {
        // Oak (idx 3, temp_opt 15°C / precip_opt 115cm) must be commoner in a
        // climate near its optimum than in a cold one (Eq. 11 scaling growth +
        // seeding). NB: compare against oak's *favourable* temperate range, not a
        // hotter tropical climate — there it is legitimately out-competed (and
        // shaded out) by the tropical broadleaf, which is emergent, not a bug.
        let oak = 3;
        let cold = grown(Climate { temp: -3.0, precip: 60.0 }, 220).species_counts()[oak];
        let warm = grown(Climate { temp: 15.0, precip: 115.0 }, 220).species_counts()[oak];
        assert!(warm > cold, "oak should be commoner near its optimum: warm {warm} vs cold {cold}");
    }

    #[test]
    fn forest_canopy_stays_upright() {
        // Regression for the banana/loop bug: in a grown stand the tall plants
        // must rise roughly over their bases, not arc over. apex_lean is the
        // highest node's horizontal offset / height.
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
        assert!(!leans.is_empty(), "expected some tall plants");
        let mean = leans.iter().sum::<f32>() / leans.len() as f32;
        assert!(mean < 0.25, "forest canopy is arcing over: mean apex_lean {mean:.2}");
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
