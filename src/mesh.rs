//! Generalized-cylinder surface mesh from a branch skeleton (Sec. 5.3).
//!
//! Each segment is rendered as a truncated cone (frustum) of `sides` radial
//! facets, tapering from radius `ra` at its base to `rb` at its tip. Geometry is
//! computed in glam, then converted to three-d's cgmath vectors at the boundary.

use crate::plant::Segment;
use glam::Vec3 as GVec3;
use three_d::{vec3, CpuMesh, Indices, Positions, Srgba, Vector3};

fn g2t(v: GVec3) -> Vector3<f32> {
    vec3(v.x, v.y, v.z)
}

/// Cheap deterministic hash → [0,1), so foliage is stable across frames
/// (no flicker) yet varied per leaf.
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2654435761);
    x ^= x >> 15;
    x = x.wrapping_mul(2246822519);
    x ^= x >> 13;
    (x & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// Build a foliage mesh: a small fan of leaf quads at each twig point, fanning
/// outward around the twig direction. Per-leaf green variation via vertex
/// colors. `points` is (position, outward direction).
pub fn build_foliage_mesh(points: &[(GVec3, GVec3)], leaf_size: f32, per_cluster: usize) -> CpuMesh {
    let mut positions: Vec<Vector3<f32>> = Vec::new();
    let mut normals: Vec<Vector3<f32>> = Vec::new();
    let mut colors: Vec<Srgba> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for (ti, (pos, dir)) in points.iter().enumerate() {
        let dir = dir.normalize_or_zero();
        let (u, v) = if dir.length_squared() > 1e-6 {
            dir.any_orthonormal_pair()
        } else {
            (GVec3::X, GVec3::Z)
        };
        for j in 0..per_cluster {
            let seed = (ti as u32).wrapping_mul(97).wrapping_add(j as u32);
            let az = std::f32::consts::TAU * (j as f32 / per_cluster as f32)
                + hash01(seed) * 1.2;
            let radial = (u * az.cos() + v * az.sin()).normalize_or_zero();
            // Leaf points outward and a bit along the twig.
            let leaf_dir = (radial + dir * 0.5).normalize_or_zero();
            let width_axis = leaf_dir.cross(radial).normalize_or_zero();
            let size = leaf_size * (0.7 + 0.6 * hash01(seed.wrapping_add(7)));

            let base_pt = *pos + dir * (leaf_size * 0.2);
            let tip = base_pt + leaf_dir * size;
            let half_w = width_axis * (size * 0.35);
            // Diamond/leaf quad: base, two side mid-points, tip.
            let mid = base_pt + leaf_dir * (size * 0.45);
            let p0 = base_pt;
            let p1 = mid + half_w;
            let p2 = tip;
            let p3 = mid - half_w;

            let normal = (p1 - p0).cross(p3 - p0).normalize_or_zero();
            let base_idx = positions.len() as u32;
            // Per-leaf brightness variation as near-white tints. The leaf
            // material's green albedo is multiplied by these, so leaves read
            // green whether or not the renderer applies vertex colors.
            let t = hash01(seed.wrapping_add(31));
            let b = 190.0 + 60.0 * t;
            let col = Srgba::new(b as u8, (b + 12.0).min(255.0) as u8, (b - 18.0).max(0.0) as u8, 255);
            for p in [p0, p1, p2, p3] {
                positions.push(g2t(p));
                normals.push(g2t(normal));
                colors.push(col);
            }
            indices.extend_from_slice(&[
                base_idx,
                base_idx + 1,
                base_idx + 2,
                base_idx,
                base_idx + 2,
                base_idx + 3,
            ]);
        }
    }

    CpuMesh {
        positions: Positions::F32(positions),
        indices: Indices::U32(indices),
        normals: Some(normals),
        colors: Some(colors),
        ..Default::default()
    }
}

pub fn build_tree_mesh(segments: &[Segment], sides: usize) -> CpuMesh {
    let sides = sides.max(3);
    let mut positions: Vec<Vector3<f32>> = Vec::new();
    let mut normals: Vec<Vector3<f32>> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for s in segments {
        let axis = s.b - s.a;
        let len = axis.length();
        if len < 1e-6 {
            continue;
        }
        let dir = axis / len;
        let (u, v) = dir.any_orthonormal_pair();
        let dr = s.rb - s.ra;
        let base = positions.len() as u32;

        for k in 0..sides {
            let ang = std::f32::consts::TAU * (k as f32) / (sides as f32);
            let radial = (u * ang.cos() + v * ang.sin()).normalize_or_zero();
            // Outward normal of the cone surface (radial tilted by the slope).
            let normal = (radial * len - dir * dr).normalize_or_zero();
            positions.push(g2t(s.a + radial * s.ra));
            normals.push(g2t(normal));
            positions.push(g2t(s.b + radial * s.rb));
            normals.push(g2t(normal));
        }

        for k in 0..sides {
            let a0 = base + 2 * k as u32; // base ring, this facet
            let b0 = base + 2 * k as u32 + 1; // tip ring, this facet
            let kn = (k + 1) % sides;
            let a1 = base + 2 * kn as u32; // base ring, next facet
            let b1 = base + 2 * kn as u32 + 1; // tip ring, next facet
            // Two triangles, counter-clockwise when viewed from outside.
            indices.extend_from_slice(&[a0, b0, b1, a0, b1, a1]);
        }
    }

    CpuMesh {
        positions: Positions::F32(positions),
        indices: Indices::U32(indices),
        normals: Some(normals),
        ..Default::default()
    }
}
