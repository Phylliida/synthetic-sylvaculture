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
    rng: ChaCha8Rng,
}

impl Ecosystem {
    pub fn new(n: usize, size: f32, seed: u64) -> Self {
        let species = species::library();
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let mut plants = Vec::with_capacity(n);
        let mut species_idx = Vec::with_capacity(n);

        for _ in 0..n {
            let x = rng.gen_range(-size..size);
            let z = rng.gen_range(-size..size);
            let si = rng.gen_range(0..species.len());
            let mut plant = Plant::new(default_library(), species[si].params.clone(), vec3(x, 0.0, z));
            // Stagger ages so the stand is not perfectly synchronized.
            let head_start = rng.gen_range(0..50);
            for _ in 0..head_start {
                plant.step(1.0);
            }
            plants.push(plant);
            species_idx.push(si);
        }

        Ecosystem {
            species,
            plants,
            species_idx,
            size,
            age: 0.0,
            shadow_enabled: true,
            rng,
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.age += dt;

        if !self.shadow_enabled {
            for p in &mut self.plants {
                p.step(dt);
            }
            return;
        }

        // 1. Deposit every module's shadow into a shared grid (current geometry).
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
