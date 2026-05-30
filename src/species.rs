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

pub fn library() -> Vec<Species> {
    vec![
        // Shade tolerance encodes successional role: pioneers (poplar/birch/
        // shrub) are sun-loving and fast; the conifer is a shade-tolerant climax.
        Species {
            name: "conifer (spruce-like)",
            params: preset(0.90, 0.90, 0.25, 130.0, 1.0, -0.20, 0.60),
            leaf_rgb: (42, 92, 56),
            bark_rgb: (96, 70, 52),
        },
        Species {
            name: "poplar (columnar)",
            params: preset(0.82, 0.70, 0.34, 110.0, 1.1, -0.15, 0.20),
            leaf_rgb: (112, 168, 72),
            bark_rgb: (122, 112, 92),
        },
        Species {
            name: "birch",
            params: preset(0.62, 0.50, 0.40, 95.0, 1.0, -0.25, 0.25),
            leaf_rgb: (146, 188, 82),
            bark_rgb: (212, 208, 198),
        },
        Species {
            name: "oak (broad)",
            params: preset(0.42, 0.30, 0.30, 120.0, 1.15, -0.30, 0.45),
            leaf_rgb: (80, 130, 55),
            bark_rgb: (92, 72, 55),
        },
        Species {
            name: "shrub",
            params: preset(0.30, 0.20, 0.50, 45.0, 0.9, -0.10, 0.15),
            leaf_rgb: (120, 150, 70),
            bark_rgb: (100, 85, 60),
        },
    ]
}
