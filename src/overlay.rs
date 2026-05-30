//! Clickable Whittaker biome chart (the temperature–precipitation diagram,
//! Fig. 2), drawn as a 2D screen overlay. Click it to set the ecosystem climate.
//!
//! Coordinates: the chart rectangle is defined in screen pixels (top-left
//! origin, matching mouse `position`). For drawing we flip Y into three-d's 2D
//! camera space (bottom-left origin) via `world_y = viewport_height − screen_y`.

use three_d::{vec3, CpuMesh, Indices, Positions, Srgba, Vector3, Viewport};

pub const T_MIN: f32 = -10.0;
pub const T_MAX: f32 = 30.0;
pub const P_MIN: f32 = 10.0;
pub const P_MAX: f32 = 400.0;

/// Chart rectangle in screen pixels: (x0, y0, width, height), top-left origin.
pub fn chart_rect(vp: Viewport) -> (f32, f32, f32, f32) {
    let margin = 18.0;
    let w = 300.0_f32.min(vp.width as f32 * 0.42);
    let h = 230.0_f32.min(vp.height as f32 * 0.5);
    (margin, margin, w, h)
}

/// Upper precipitation bound of the realizable (triangular) climate region:
/// cold-and-very-wet does not occur, so the chart tapers to a triangle.
fn precip_max_for(temp: f32) -> f32 {
    let f = ((temp - T_MIN) / (T_MAX - T_MIN)).clamp(0.0, 1.0);
    130.0 + f * (P_MAX - 130.0)
}

/// Biome colour for a climate point — mirrors `ecosystem::biome_name`'s regions.
pub fn biome_color(t: f32, p: f32) -> [u8; 3] {
    if t < 0.0 {
        [182, 197, 212] // tundra
    } else if t < 7.0 {
        if p < 40.0 {
            [200, 190, 140] // cold desert / grassland
        } else {
            [46, 92, 82] // boreal forest
        }
    } else if t < 20.0 {
        if p < 40.0 {
            [182, 196, 96] // temperate grassland
        } else if p < 100.0 {
            [86, 150, 70] // temperate seasonal forest
        } else {
            [42, 120, 92] // temperate rainforest
        }
    } else if p < 50.0 {
        [222, 202, 150] // subtropical desert
    } else if p < 150.0 {
        [176, 168, 82] // savanna
    } else {
        [34, 112, 52] // tropical rainforest
    }
}

/// Map a mouse position (screen pixels) to a climate, if it is inside the chart.
pub fn screen_to_climate(vp: Viewport, px: f32, py: f32) -> Option<(f32, f32)> {
    let (x0, y0, w, h) = chart_rect(vp);
    if px < x0 || px > x0 + w || py < y0 || py > y0 + h {
        return None;
    }
    let fx = (px - x0) / w;
    let fy = (py - y0) / h; // 0 at top of chart
    let temp = T_MIN + fx * (T_MAX - T_MIN);
    let precip = P_MIN + (1.0 - fy) * (P_MAX - P_MIN); // top = high precip
    Some((temp, precip))
}

/// Build the overlay mesh (background panel, biome cells, current-climate
/// marker) for the given viewport and climate. Vertex-coloured, unlit.
pub fn build_chart(vp: Viewport, temp: f32, precip: f32) -> CpuMesh {
    let (x0, y0, w, h) = chart_rect(vp);
    let vh = vp.height as f32;

    let mut positions: Vec<Vector3<f32>> = Vec::new();
    let mut colors: Vec<Srgba> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Screen-space rectangle (top-left origin) → two triangles, Y flipped.
    let mut quad = |sx: f32, sy: f32, sw: f32, sh: f32, c: [u8; 3]| {
        let base = positions.len() as u32;
        let col = Srgba::new(c[0], c[1], c[2], 255);
        for (cx, cy) in [(sx, sy), (sx + sw, sy), (sx + sw, sy + sh), (sx, sy + sh)] {
            positions.push(vec3(cx, vh - cy, 0.0));
            colors.push(col);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    // Background panel / frame.
    quad(x0 - 5.0, y0 - 5.0, w + 10.0, h + 10.0, [26, 28, 34]);

    // Biome cells over the triangular realizable region.
    let nx = 20usize;
    let ny = 18usize;
    let cw = w / nx as f32;
    let ch = h / ny as f32;
    for i in 0..nx {
        for j in 0..ny {
            let fx = (i as f32 + 0.5) / nx as f32;
            let fy = (j as f32 + 0.5) / ny as f32;
            let t = T_MIN + fx * (T_MAX - T_MIN);
            let p = P_MIN + (1.0 - fy) * (P_MAX - P_MIN);
            if p > precip_max_for(t) {
                continue; // outside the triangle
            }
            quad(
                x0 + i as f32 * cw,
                y0 + j as f32 * ch,
                cw + 0.6,
                ch + 0.6,
                biome_color(t, p),
            );
        }
    }

    // Current-climate marker (dark halo + white dot).
    let mfx = ((temp - T_MIN) / (T_MAX - T_MIN)).clamp(0.0, 1.0);
    let mfy = (1.0 - (precip - P_MIN) / (P_MAX - P_MIN)).clamp(0.0, 1.0);
    let mx = x0 + mfx * w;
    let my = y0 + mfy * h;
    quad(mx - 6.0, my - 6.0, 12.0, 12.0, [15, 15, 15]);
    quad(mx - 4.0, my - 4.0, 8.0, 8.0, [255, 255, 255]);

    CpuMesh {
        positions: Positions::F32(positions),
        indices: Indices::U32(indices),
        colors: Some(colors),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use three_d::Viewport;

    fn vp() -> Viewport {
        Viewport { x: 0, y: 0, width: 1280, height: 800 }
    }

    #[test]
    fn click_inverts_marker_mapping() {
        // For a climate, the marker sits at some screen point; clicking that
        // point must return (approximately) the same climate.
        let v = vp();
        let (x0, y0, w, h) = chart_rect(v);
        for (t, p) in [(-5.0, 30.0), (10.0, 90.0), (24.0, 250.0)] {
            let mfx = (t - T_MIN) / (T_MAX - T_MIN);
            let mfy = 1.0 - (p - P_MIN) / (P_MAX - P_MIN);
            let (px, py) = (x0 + mfx * w, y0 + mfy * h);
            let (t2, p2) = screen_to_climate(v, px, py).expect("inside chart");
            assert!((t - t2).abs() < 0.01, "temp {t} -> {t2}");
            assert!((p - p2).abs() < 0.1, "precip {p} -> {p2}");
        }
    }

    #[test]
    fn clicks_outside_chart_are_ignored() {
        let v = vp();
        assert!(screen_to_climate(v, 1000.0, 700.0).is_none()); // far from top-left chart
        assert!(screen_to_climate(v, 5.0, 5.0).is_none()); // just outside the margin
    }

    #[test]
    fn warm_wet_corner_is_tropical_cold_is_tundra() {
        // Chart colour regions should match the climate semantics.
        assert_eq!(biome_color(26.0, 300.0), [34, 112, 52]); // tropical rainforest
        assert_eq!(biome_color(-5.0, 30.0), [182, 197, 212]); // tundra
    }
}
