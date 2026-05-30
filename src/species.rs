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
    /// Lifespan: senescence onset (p_max); fully senesced ≈ 2·max_age.
    pub max_age: f32,
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
    beta: f32,
    g2: f32,
    shade_tolerance: f32,
) -> PlantParams {
    PlantParams {
        lambda,
        determinacy,
        gp,
        v_root_max,
        v_max: v_root_max,
        beta,
        g2,
        shade_tolerance,
        // The viewer's standard environment-sensitive model.
        collision_light: true,
        optimize_orientation: true,
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
    params.p_max = max_age; // senescence onset
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
        max_age,
    }
}

pub fn library() -> Vec<Species> {
    // Shade tolerance encodes successional role (pioneers sun-loving and fast,
    // conifer a shade-tolerant climax); climate optima place each species along
    // the temperature–precipitation axes of the biome diagram (Fig. 2).
    //   preset(λ, D, gp, v_root_max, β, g2, s_tol)
    //   species(.., temp_opt, precip_opt, t_σ, p_σ, flower_age, seed_r, seed_freq, max_age)
    vec![
        species(
            "conifer (spruce-like)",
            preset(0.90, 0.90, 0.25, 130.0, 1.0, -0.20, 0.60),
            (42, 92, 56), (96, 70, 52),
            2.0, 80.0, 11.0, 90.0, 80.0, 5.0, 0.030, 250.0,
        ),
        species(
            "poplar (columnar)",
            preset(0.82, 0.70, 0.34, 110.0, 1.1, -0.15, 0.20),
            (112, 168, 72), (122, 112, 92),
            13.0, 95.0, 10.0, 80.0, 55.0, 8.0, 0.055, 150.0,
        ),
        species(
            "birch",
            preset(0.62, 0.50, 0.40, 95.0, 1.0, -0.25, 0.25),
            (146, 188, 82), (212, 208, 198),
            8.0, 70.0, 11.0, 80.0, 50.0, 7.0, 0.060, 130.0,
        ),
        species(
            "oak (broad)",
            preset(0.42, 0.30, 0.30, 120.0, 1.15, -0.30, 0.45),
            (80, 130, 55), (92, 72, 55),
            15.0, 115.0, 10.0, 90.0, 70.0, 5.0, 0.035, 320.0,
        ),
        species(
            "shrub",
            preset(0.30, 0.20, 0.50, 45.0, 0.9, -0.10, 0.15),
            (120, 150, 70), (100, 85, 60),
            6.0, 40.0, 16.0, 150.0, 28.0, 6.0, 0.090, 80.0,
        ),
        // Warm-end species so the savanna/tropical biomes are populated.
        species(
            "acacia (savanna)",
            preset(0.45, 0.35, 0.34, 95.0, 1.25, -0.28, 0.30),
            (150, 170, 80), (110, 95, 70),
            24.0, 55.0, 9.0, 60.0, 50.0, 9.0, 0.055, 200.0,
        ),
        species(
            "tropical broadleaf",
            preset(0.50, 0.40, 0.42, 145.0, 1.2, -0.25, 0.55),
            (54, 150, 58), (95, 75, 55),
            26.0, 320.0, 8.0, 120.0, 60.0, 6.0, 0.055, 300.0,
        ),
    ]
}
