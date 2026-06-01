//! Evolvable plant genome (ecosystem mode).
//!
//! Instead of a fixed library of hand-tuned species, an evolving ecosystem
//! starts from a handful of **founders with uniform-random genomes** and lets
//! selection do the specializing. A genome is a continuous vector of heritable
//! traits — morphology + life-history. Seeds **inherit the parent genome with a
//! small Gaussian mutation**, so lineages can climb toward whatever morphology
//! pays off in the local climate.
//!
//! Climate is deliberately **not** in the genome and there is no hardcoded
//! niche: the environment couples to growth only through a single physical
//! productivity scalar `P` (see `Climate::productivity` in `ecosystem.rs`), and
//! specialization to a biome is purely emergent — different trait combinations
//! genuinely perform differently under different `P`, and differential survival
//! and reproduction concentrate the winners.

use crate::plant::PlantParams;
use rand::Rng;

/// Inclusive `[min, max]` range of a heritable trait. `random` draws uniformly
/// within it; `mutate` jitters and clamps back to it.
#[derive(Clone, Copy)]
struct Range(f32, f32);

impl Range {
    fn draw(self, rng: &mut impl Rng) -> f32 {
        rng.gen_range(self.0..=self.1)
    }
    /// Jitter by a Gaussian-ish step scaled to the trait's span, then clamp.
    fn jitter(self, v: f32, rate: f32, rng: &mut impl Rng) -> f32 {
        // Triangular noise (sum of two uniforms) ≈ a cheap bell, no extra deps.
        let n = (rng.gen::<f32>() - rng.gen::<f32>()) * (self.1 - self.0) * rate;
        (v + n).clamp(self.0, self.1)
    }
}

/// The heritable traits. Ranges are the trait's evolvable bounds (also the
/// uniform-random founder range). Keep `RANGES` and the fields in the same
/// order — `as_array`/`from_array` rely on it.
#[derive(Clone, Debug)]
pub struct Genome {
    // --- morphology (shape, rate, allocation) ---
    pub lambda: f32,          // apical control λ (≈0.5 spans excurrent↔decurrent)
    pub determinacy: f32,     // lateral branch angle (high = narrow/upright)
    pub alpha: f32,           // resource coefficient (growth vigor)
    pub gp: f32,              // growth-rate multiplier
    pub v_root_max: f32,      // resource budget / size ceiling
    pub g2: f32,              // lateral gravitropism (− droops, + lifts)
    pub tropism_up: f32,      // leader up-righting (straight bole)
    pub xi: f32,              // optimal-dir weight (low = straight, high = gnarled)
    pub phi: f32,             // pipe-model tip diameter (trunk thickness)
    pub shade_tolerance: f32, // light floor (climax tolerance)
    pub shed_ratio: f32,      // branch-shedding threshold
    pub envelope_height: f32, // crown silhouette height (max height)
    pub envelope_radius: f32, // crown silhouette radius (max spread)
    // --- life history (reproductive strategy: r ↔ K) ---
    pub flowering_age: f32, // age (steps) before it can seed
    pub seed_radius: f32,   // seed scatter std-dev around the parent
    pub seed_freq: f32,     // per-step seeding probability when thriving
    pub lifespan: f32,      // senescence onset p_max; the plant dies of old age
                            // around 1.9× this. Finite ⇒ even canopy winners
                            // eventually die and open gaps, so the population
                            // churns and selection keeps compounding. Short =
                            // live-fast-seed-early (r), long = grow-tall-late (K).
}

/// Evolvable bounds, in field order. Chosen to span the qualitative morphospace
/// (Pałubicki Fig. 7/12) without admitting degenerate plants.
const RANGES: [Range; 17] = [
    Range(0.35, 0.65),  // lambda
    Range(0.0, 1.0),    // determinacy
    Range(1.2, 3.0),    // alpha
    Range(0.6, 1.4),    // gp
    Range(40.0, 220.0), // v_root_max
    Range(-0.35, 0.15), // g2
    Range(0.05, 0.50),  // tropism_up
    Range(0.10, 0.60),  // xi
    Range(0.03, 0.07),  // phi
    Range(0.0, 0.8),    // shade_tolerance
    Range(0.20, 0.50),  // shed_ratio
    Range(4.0, 30.0),   // envelope_height
    Range(2.0, 8.0),    // envelope_radius
    Range(20.0, 90.0),  // flowering_age
    Range(4.0, 16.0),   // seed_radius
    Range(0.02, 0.12),  // seed_freq
    Range(80.0, 780.0), // lifespan (p_max) — death ≈ 1.9×, so ~152–1480 steps;
                        // long enough that a plant reliably seeds before it dies,
                        // but finite so the canopy still turns over (gap churn).
];

impl Genome {
    fn as_array(&self) -> [f32; 17] {
        [
            self.lambda, self.determinacy, self.alpha, self.gp, self.v_root_max,
            self.g2, self.tropism_up, self.xi, self.phi, self.shade_tolerance,
            self.shed_ratio, self.envelope_height, self.envelope_radius,
            self.flowering_age, self.seed_radius, self.seed_freq, self.lifespan,
        ]
    }

    fn from_array(a: [f32; 17]) -> Self {
        Genome {
            lambda: a[0], determinacy: a[1], alpha: a[2], gp: a[3], v_root_max: a[4],
            g2: a[5], tropism_up: a[6], xi: a[7], phi: a[8], shade_tolerance: a[9],
            shed_ratio: a[10], envelope_height: a[11], envelope_radius: a[12],
            flowering_age: a[13], seed_radius: a[14], seed_freq: a[15], lifespan: a[16],
        }
    }

    /// The trait vector (field order), for aggregation / readout.
    pub fn traits(&self) -> [f32; 17] {
        self.as_array()
    }

    /// Trait names aligned with `traits()` (for `--stats` readout / debugging).
    #[allow(dead_code)]
    pub const NAMES: [&'static str; 17] = [
        "lambda", "determ", "alpha", "gp", "v_root_max", "g2", "tropism_up", "xi",
        "phi", "shade_tol", "shed", "env_h", "env_r", "flower_age", "seed_r", "seed_freq",
        "lifespan",
    ];

    /// A founder: every trait drawn uniformly at random within its range.
    pub fn random(rng: &mut impl Rng) -> Self {
        let mut a = [0.0f32; 17];
        for (i, r) in RANGES.iter().enumerate() {
            a[i] = r.draw(rng);
        }
        Genome::from_array(a)
    }

    /// A heritable copy: the parent's traits with a small Gaussian mutation per
    /// trait (`rate` ≈ fraction of each trait's span as the step scale).
    pub fn mutated(&self, rate: f32, rng: &mut impl Rng) -> Self {
        let cur = self.as_array();
        let mut a = [0.0f32; 17];
        for (i, r) in RANGES.iter().enumerate() {
            a[i] = r.jitter(cur[i], rate, rng);
        }
        Genome::from_array(a)
    }

    /// Ecological niche descriptor, each axis normalized to [0,1]: crown height,
    /// crown breadth, shade strategy — the axes along which plants overlap in how
    /// they use light and space. Two plants close here occupy the same niche and
    /// so compete most (used for negative frequency-dependence; also a natural
    /// quality-diversity "behavior characterization").
    pub fn niche(&self) -> [f32; 3] {
        let n = |v: f32, lo: f32, hi: f32| ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
        [
            n(self.envelope_height, 4.0, 30.0),
            n(self.envelope_radius, 2.0, 8.0),
            n(self.shade_tolerance, 0.0, 0.8),
        ]
    }

    /// Build the runtime `PlantParams` this genome expresses. The marker budget
    /// and module cap are *derived* from the crown volume, so a bigger-envelope
    /// genome automatically gets the budget to fill it (and a small one stays
    /// cheap — which also keeps tiny scrub fast to simulate).
    pub fn to_params(&self) -> PlantParams {
        let vol = std::f32::consts::PI * self.envelope_radius * self.envelope_radius * self.envelope_height;
        let markers = (vol * 1.4).clamp(300.0, 2600.0) as usize;
        let modules = (vol * 1.6).clamp(300.0, 2600.0) as usize;
        PlantParams {
            lambda: self.lambda,
            determinacy: self.determinacy,
            alpha: self.alpha,
            gp: self.gp,
            v_root_max: self.v_root_max,
            g2: self.g2,
            tropism_up: self.tropism_up,
            xi: self.xi,
            phi: self.phi,
            shade_tolerance: self.shade_tolerance,
            shed_ratio: self.shed_ratio,
            envelope_height: self.envelope_height,
            envelope_radius: self.envelope_radius,
            p_max: self.lifespan, // finite lifespan → senescence → gap churn
            marker_count: markers,
            max_modules: modules,
            ..PlantParams::default()
        }
    }

    /// Leaf colour derived from the genome, so visually-similar strategies share
    /// a colour and a specializing biome is *seen* to converge. Hue tracks crown
    /// slenderness (short-broad → yellow-green, tall-narrow → blue-green);
    /// brightness tracks shade tolerance (pioneer bright → climax dark).
    pub fn leaf_rgb(&self) -> [u8; 3] {
        let slender = (self.envelope_height / (self.envelope_radius * 2.0)).clamp(0.4, 3.0);
        let hue = lerp(75.0, 158.0, (slender - 0.4) / 2.6); // yellow-green → teal-green
        let val = lerp(0.82, 0.42, self.shade_tolerance.clamp(0.0, 1.0));
        hsv_to_rgb(hue, 0.62, val)
    }

    /// Bark colour: thicker/larger genomes read darker and browner.
    pub fn bark_rgb(&self) -> [u8; 3] {
        let big = ((self.v_root_max - 40.0) / 180.0).clamp(0.0, 1.0);
        let val = lerp(0.52, 0.30, big);
        hsv_to_rgb(28.0, 0.45, val)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// HSV (h in degrees, s,v in [0,1]) → 8-bit RGB.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}
