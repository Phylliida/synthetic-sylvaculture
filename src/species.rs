//! Plant-type presets (Sec. 6.1, Tab. 4). The paper tabulates 16 species by
//! (pmax, v̄rootmax, gp, λ, D, Fage, α, ω2, g1, φ, β). Those values are in the
//! paper's internal units, which differ from this reproduction's scales, so
//! these presets are *adapted* — they keep the qualitative character of the
//! morphospace (apical control λ, determinacy D, growth rate, tropism) and the
//! distinct silhouette/colour of each form, rather than transcribing the table
//! verbatim. Cycle them in the viewer with N.

use crate::plant::PlantParams;

pub struct Species {
    pub name: &'static str,
    pub params: PlantParams,
    pub leaf_rgb: (u8, u8, u8),
    pub bark_rgb: (u8, u8, u8),
    // --- ecosystem traits (Secs. 6.3, 6.4) ---
    /// Optimal climate (Eq. 11): temperature °C and precipitation cm.
    pub temp_opt: f32,
    pub precip_opt: f32,
    pub temp_sigma: f32,
    pub precip_sigma: f32,
    /// Plant age (steps) at which it can begin seeding.
    pub flowering_age: f32,
    /// Std-dev of the Gaussian seed scatter around the parent (world units).
    pub seed_radius: f32,
    /// Per-plant per-step seeding probability (before climate scaling).
    pub seed_freq: f32,
}

impl Species {
    /// Climate adaptation o ∈ [0,1] (Eq. 11): product of temperature and
    /// precipitation Gaussians centred on the species optima.
    pub fn adaptation(&self, temp: f32, precip: f32) -> f32 {
        let dt = (temp - self.temp_opt) / self.temp_sigma;
        let dp = (precip - self.precip_opt) / self.precip_sigma;
        (-0.5 * dt * dt).exp() * (-0.5 * dp * dp).exp()
    }
}

#[allow(clippy::too_many_arguments)]
fn preset(
    lambda: f32,
    determinacy: f32,
    gp: f32,
    v_root_max: f32,
    g2: f32,
    shade_tolerance: f32,
    phi: f32,
) -> PlantParams {
    PlantParams {
        lambda,           // apical control λ (≈0.5 spans excurrent↔decurrent)
        determinacy,      // → lateral branch angle (high = narrow, excurrent)
        gp,               // growth-rate multiplier on the resource
        v_root_max,       // resource budget / size cap (climate scales it)
        g2,               // lateral gravitropism (− droops branches, + lifts)
        shade_tolerance,
        phi,              // pipe-model tip diameter (sets trunk thickness)
        max_modules: 600, // per-plant safety cap on metamers
        ..PlantParams::default()
    }
}

#[allow(clippy::too_many_arguments)]
fn species(
    name: &'static str,
    params: PlantParams,
    leaf_rgb: (u8, u8, u8),
    bark_rgb: (u8, u8, u8),
    temp_opt: f32,
    precip_opt: f32,
    temp_sigma: f32,
    precip_sigma: f32,
    flowering_age: f32,
    seed_radius: f32,
    seed_freq: f32,
    max_age: f32,
) -> Species {
    let mut params = params;
    params.p_max = max_age; // senescence onset (fully senesced ≈ 2·max_age)
    Species {
        name,
        params,
        leaf_rgb,
        bark_rgb,
        temp_opt,
        precip_opt,
        temp_sigma,
        precip_sigma,
        flowering_age,
        seed_radius,
        seed_freq,
    }
}

pub fn library() -> Vec<Species> {
    // Shade tolerance encodes successional role (pioneers sun-loving and fast,
    // conifer a shade-tolerant climax); climate optima place each species along
    // the temperature–precipitation axes of the biome diagram (Fig. 2).
    //   preset(λ, D, gp, v_root_max, g2, s_tol, phi)
    //   species(.., temp_opt, precip_opt, t_σ, p_σ, flower_age, seed_r, seed_freq, max_age)
    vec![
        species(
            "conifer (spruce-like)",
            // Excurrent: high apical control λ, narrow whorls (high D), gentle
            // droop (g2). The conical spire emerges; no per-species hacks.
            preset(0.53, 0.72, 1.0, 150.0, -0.22, 0.60, 0.050),
            (42, 92, 56), (96, 70, 52),
            2.0, 80.0, 11.0, 90.0, 80.0, 5.0, 0.030, 250.0,
        ),
        species(
            "poplar (columnar)",
            // More excurrent still, narrow near-vertical laterals (high D),
            // little droop — a slender column emerges from λ/D/g2 alone.
            preset(0.55, 0.85, 1.0, 120.0, -0.05, 0.20, 0.050),
            (112, 168, 72), (122, 112, 92),
            13.0, 95.0, 10.0, 80.0, 55.0, 8.0, 0.055, 150.0,
        ),
        species(
            "birch",
            preset(0.50, 0.50, 1.0, 80.0, -0.16, 0.25, 0.045),
            (146, 188, 82), (212, 208, 198),
            8.0, 70.0, 11.0, 80.0, 50.0, 7.0, 0.060, 130.0,
        ),
        species(
            "oak (broad)",
            // Broad decurrent crown: low λ, wide laterals, pronounced droop.
            preset(0.47, 0.32, 1.0, 95.0, -0.22, 0.45, 0.052),
            (80, 130, 55), (92, 72, 55),
            15.0, 115.0, 10.0, 90.0, 70.0, 5.0, 0.035, 320.0,
        ),
        species(
            "shrub",
            preset(0.45, 0.22, 1.1, 45.0, -0.12, 0.15, 0.045),
            (120, 150, 70), (100, 85, 60),
            6.0, 40.0, 16.0, 150.0, 28.0, 6.0, 0.090, 80.0,
        ),
        // Warm-end species so the savanna/tropical biomes are populated.
        species(
            "acacia (savanna)",
            // Flat-topped: wide laterals with strong droop levelling the crown.
            preset(0.48, 0.38, 1.0, 90.0, -0.30, 0.30, 0.052),
            (150, 170, 80), (110, 95, 70),
            24.0, 55.0, 9.0, 60.0, 50.0, 9.0, 0.055, 200.0,
        ),
        species(
            "tropical broadleaf",
            preset(0.50, 0.45, 1.0, 130.0, -0.20, 0.55, 0.052),
            (54, 150, 58), (95, 75, 55),
            26.0, 320.0, 8.0, 120.0, 60.0, 6.0, 0.055, 300.0,
        ),
    ]
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::plant::Plant;
    use glam::Vec3;

    // Library order: 0 conifer, 1 poplar, 2 birch, 3 oak, 4 shrub,
    //                5 acacia, 6 tropical broadleaf.
    fn grow_solo(idx: usize, steps: u32) -> Plant {
        let lib = library();
        let mut p = Plant::new(lib[idx].params.clone(), Vec3::ZERO);
        for _ in 0..steps {
            p.step(1.0);
        }
        p
    }

    fn trunk_radius(p: &Plant) -> f32 {
        p.skeleton().iter().map(|s| s.ra).fold(0.0, f32::max)
    }
    fn slenderness(p: &Plant) -> f32 {
        let (h, _, _) = p.shape();
        h / (2.0 * trunk_radius(p)).max(1e-3)
    }
    fn spread(p: &Plant) -> f32 {
        let (h, crown, _) = p.shape();
        crown / h.max(1e-3)
    }
    fn apex_lean(p: &Plant) -> f32 {
        let (h, _, apex) = p.shape();
        apex / h.max(1e-3)
    }

    // These tests pin "how each species should look" so the parameters can be
    // tuned against them. The numbers come from the --stats morphology readout
    // with comfortable margins.

    #[test]
    fn excurrent_species_tower_over_broad_ones() {
        // The excurrent species (conifer, poplar) should be much taller AND
        // narrower than the broad decurrent oak — the headline λ contrast.
        let conifer = grow_solo(0, 120);
        let poplar = grow_solo(1, 120);
        let oak = grow_solo(3, 120);
        let oh = oak.shape().0;
        assert!(conifer.shape().0 > 2.0 * oh, "conifer should tower over oak ({oh:.1})");
        assert!(poplar.shape().0 > 2.0 * oh, "poplar should tower over oak ({oh:.1})");
        assert!(spread(&conifer) < spread(&oak), "conifer should be narrower than oak");
        assert!(spread(&poplar) < spread(&oak), "poplar should be narrower than oak");
    }

    #[test]
    fn every_species_carries_foliage() {
        // Sanity: every species develops actual foliage (not a dead bare stick).
        // The metamer model lets excurrent species be narrow columns, so we no
        // longer pin a crown-spread band — only that leaves exist.
        for idx in 0..library().len() {
            let p = grow_solo(idx, 150);
            assert!(p.leaves().len() > 5, "species {idx} has almost no foliage");
        }
    }

    #[test]
    fn excurrent_leaders_stay_straight() {
        // The excurrent species (conifer, poplar) develop a dominant vertical
        // leader, so their highest point stays over the base (no banana/loop).
        // Decurrent species legitimately spread, so this only pins the leaders.
        for idx in [0usize, 1] {
            let lean = apex_lean(&grow_solo(idx, 150));
            assert!(lean < 0.25, "excurrent species {idx} leader arcs (apex_lean {lean:.2})");
        }
    }

    #[test]
    fn bigger_species_have_thicker_trunks() {
        // Pipe Model: a much larger tree carries more leaves -> a thicker trunk.
        let conifer = trunk_radius(&grow_solo(0, 150));
        let shrub = trunk_radius(&grow_solo(4, 150));
        assert!(conifer > 1.7 * shrub, "conifer trunk {conifer} vs shrub {shrub}");
    }

    #[test]
    fn trunk_thickens_as_the_tree_grows() {
        // A young tree (few leaves) must have a thinner trunk than the same tree
        // once grown (Pipe Model). Baseline taken very early, before it fills out.
        let mut p = grow_solo(0, 4);
        let early = trunk_radius(&p);
        for _ in 0..120 {
            p.step(1.0);
        }
        let late = trunk_radius(&p);
        assert!(late > early * 1.3, "trunk should thicken over time: {early:.3} -> {late:.3}");
    }

    #[test]
    fn climate_adaptation_is_a_gaussian_niche_peaking_at_the_optimum() {
        // Eq. 11: o(T,P) is a product of temperature/precipitation Gaussians,
        // so it is exactly 1 at the species optimum and decays monotonically as
        // the climate departs from it in either axis.
        for sp in library() {
            let peak = sp.adaptation(sp.temp_opt, sp.precip_opt);
            assert!((peak - 1.0).abs() < 1e-5, "{} peak adaptation {peak} != 1", sp.name);
            // One sigma off in temperature ⇒ exp(-0.5) ≈ 0.607 of the peak.
            let one_sigma = sp.adaptation(sp.temp_opt + sp.temp_sigma, sp.precip_opt);
            assert!((one_sigma - (-0.5f32).exp()).abs() < 1e-4, "{} 1σ temp", sp.name);
            // Monotone decay: further from the optimum is never higher.
            let near = sp.adaptation(sp.temp_opt + 3.0, sp.precip_opt + 5.0);
            let far = sp.adaptation(sp.temp_opt + 12.0, sp.precip_opt + 40.0);
            assert!(far < near, "{}: adaptation should fall off with distance", sp.name);
            assert!((0.0..=1.0).contains(&far), "{} adaptation out of [0,1]", sp.name);
        }
    }

    #[test]
    fn trunk_radii_and_slenderness_stay_finite() {
        // Sanity (not aesthetics): a grown tree has a real, non-degenerate
        // trunk. The metamer model favours faithfulness over a tidy silhouette,
        // so excurrent species can be very slender (a tall column) — we only
        // bar pencil-thin/infinite or stumpy degeneracies.
        for idx in 0..library().len() {
            let p = grow_solo(idx, 150);
            let r = trunk_radius(&p);
            let s = slenderness(&p);
            assert!(r > 0.04, "species {idx} trunk too thin: {r:.3}");
            assert!((2.0..=120.0).contains(&s), "species {idx} slenderness {s:.0} degenerate");
        }
    }
}
