//! Generalized-cylinder surface mesh from a branch skeleton (Sec. 5.3).
//!
//! Each segment is rendered as a truncated cone (frustum) of `sides` radial
//! facets, tapering from radius `ra` at its base to `rb` at its tip. Geometry is
//! computed in glam, then converted to three-d's cgmath vectors at the boundary.

use crate::plant::Segment;
use glam::Vec3 as GVec3;
use three_d::{vec3, CpuMesh, Indices, Positions, Vector3};

fn g2t(v: GVec3) -> Vector3<f32> {
    vec3(v.x, v.y, v.z)
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
