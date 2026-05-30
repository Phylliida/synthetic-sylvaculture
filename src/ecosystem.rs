//! Ecosystem scale (Sec. 6): many plants growing together on a terrain.
//!
//! E1 (this stage): multiple plants of mixed species scattered on flat ground,
//! each an independent growth simulation, rendered as one combined mesh.
//! Global shadowing, seeding, and climate arrive in later stages.

use crate::plant::{Plant, Segment};
use crate::prototype::default_library;
use crate::species::{self, Species};
use glam::{vec3, Vec3};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

pub struct Ecosystem {
    pub species: Vec<Species>,
    pub plants: Vec<Plant>,
    /// Species index of each plant (parallel to `plants`).
    pub species_idx: Vec<usize>,
    /// Half-extent of the square ground.
    pub size: f32,
    pub age: f32,
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
            rng,
        }
    }

    pub fn step(&mut self, dt: f32) {
        self.age += dt;
        for p in &mut self.plants {
            p.step(dt);
        }
    }

    pub fn plant_count(&self) -> usize {
        self.plants.len()
    }

    pub fn total_modules(&self) -> usize {
        self.plants.iter().map(|p| p.module_count()).sum()
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
