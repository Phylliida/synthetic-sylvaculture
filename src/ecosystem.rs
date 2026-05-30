//! Ecosystem scale (Sec. 6): many plants growing together on a terrain.
//!
//! E1 (this stage): multiple plants of mixed species scattered on flat ground,
//! each an independent growth simulation, rendered as one combined mesh.
//! Global shadowing, seeding, and climate arrive in later stages.

use crate::plant::{ModuleId, Plant, Segment};
use crate::prototype::default_library;
use crate::species::{self, Species};
use glam::{vec3, Vec3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;

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

    fn in_bounds(&self, i: i32, j: i32, k: i32) -> bool {
        i >= 0
            && j >= 0
            && k >= 0
            && (i as usize) < self.nx
            && (j as usize) < self.ny
            && (k as usize) < self.nz
    }

    fn idx(&self, i: i32, j: i32, k: i32) -> usize {
        i as usize + self.nx * (k as usize + self.nz * j as usize)
    }

    fn deposit(&mut self, p: Vec3) {
        let (ci, cj, ck) = self.ijk(p);
        for q in 0..=self.qmax {
            let j = cj - q; // shadow propagates downward
            if j < 0 {
                break;
            }
            let ds = self.a * self.b.powi(-q);
            for di in -q..=q {
                for dk in -q..=q {
                    let (i, k) = (ci + di, ck + dk);
                    if self.in_bounds(i, j, k) {
                        let id = self.idx(i, j, k);
                        self.s[id] += ds;
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
    /// Climate (Sec. 6.4): drives per-species adaptation o (Eq. 11).
    pub climate: Climate,
    /// Population cap (for interactive performance).
    pub max_plants: usize,
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
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut eco = Ecosystem {
            species,
            plants: Vec::new(),
            species_idx: Vec::new(),
            size,
            age: 0.0,
            shadow_enabled: true,
            climate,
            max_plants: 170,
            rng,
        };

        for _ in 0..n {
            let x = eco.rng.gen_range(-size..size);
            let z = eco.rng.gen_range(-size..size);
            let si = eco.pick_species_for_climate();
            let mut plant = eco.make_plant_of(si, vec3(x, 0.0, z));
            // Stagger ages so the stand is not perfectly synchronized.
            let head_start = eco.rng.gen_range(0..50);
            for _ in 0..head_start {
                plant.step(1.0);
            }
            eco.plants.push(plant);
            eco.species_idx.push(si);
        }
        eco
    }

    /// Build a plant of species `si` at `pos`, with its growth potential scaled
    /// by climate adaptation o (Eq. 11) — poorly-adapted species barely grow.
    fn make_plant_of(&self, si: usize, pos: Vec3) -> Plant {
        let sp = &self.species[si];
        let o = sp.adaptation(self.climate.temp, self.climate.precip);
        let mut params = sp.params.clone();
        params.v_root_max *= o; // total growth potential scales with adaptation
        Plant::new(default_library(), params, pos)
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
        self.age += dt;

        // --- growth ---
        if self.shadow_enabled {
            // 1. Deposit every module's shadow into a shared grid.
            let mut grid = ShadowGrid::new(self.size, 45.0, 1.5);
            for p in &self.plants {
                for (_, c) in p.module_centres() {
                    grid.deposit(c);
                }
            }
            // 2. Each plant reads its per-module global-shadow light and grows.
            for p in &mut self.plants {
                let qg: HashMap<ModuleId, f32> = p
                    .module_centres()
                    .into_iter()
                    .map(|(id, c)| (id, grid.light_at(c)))
                    .collect();
                p.step_shaded(dt, &qg);
            }
        } else {
            for p in &mut self.plants {
                p.step(dt);
            }
        }

        self.cull_dead();
        self.seed(dt);
    }

    /// Remove senesced or fully-suppressed plants, opening gaps (Sec. 4.2).
    fn cull_dead(&mut self) {
        let dead: Vec<bool> = self
            .plants
            .iter()
            .map(|p| {
                let senesced = p.age >= 1.9 * p.params.p_max;
                let suppressed = p.age > 50.0 && p.module_count() <= 1;
                senesced || suppressed
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

    /// Per-plant trunk segments tinted with that plant's bark colour.
    pub fn trunk_batches(&self) -> Vec<(Vec<Segment>, [u8; 3])> {
        self.plants
            .iter()
            .zip(&self.species_idx)
            .map(|(p, &si)| {
                let c = self.species[si].bark_rgb;
                (p.skeleton(), [c.0, c.1, c.2])
            })
            .collect()
    }

    /// Per-plant leaf points tinted with that plant's leaf colour.
    pub fn foliage_batches(&self) -> Vec<(Vec<(Vec3, Vec3)>, [u8; 3])> {
        self.plants
            .iter()
            .zip(&self.species_idx)
            .map(|(p, &si)| {
                let c = self.species[si].leaf_rgb;
                (p.leaves(), [c.0, c.1, c.2])
            })
            .collect()
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
        // Oak (idx 3) is warm-adapted (temp_opt 15°C); it must be commoner in a
        // warm climate than a cold one (Eq. 11 scaling growth + seeding).
        let oak = 3;
        let cold = grown(Climate { temp: -3.0, precip: 60.0 }, 220).species_counts()[oak];
        let warm = grown(Climate { temp: 24.0, precip: 200.0 }, 220).species_counts()[oak];
        assert!(warm > cold, "oak should be commoner when warm: warm {warm} vs cold {cold}");
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
