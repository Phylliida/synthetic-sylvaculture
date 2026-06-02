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

/// Cap on parallel mesh chunks (bounds thread spawns on many-core machines).
const MESH_CHUNKS: usize = 64;

fn n_mesh_chunks(n_items: usize) -> usize {
    MESH_CHUNKS
        .min(std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
        .min(n_items.max(1))
        .max(1)
}

/// Split a buffer into consecutive disjoint mutable sub-slices of the given
/// lengths (Σ lens must equal `buf.len()`), so threads can fill the final
/// vertex/index buffers in place — no per-thread allocation, no concat copy.
fn carve_mut<'a, T>(mut buf: &'a mut [T], lens: &[usize]) -> Vec<&'a mut [T]> {
    let mut out = Vec::with_capacity(lens.len());
    for &l in lens {
        let (head, tail) = buf.split_at_mut(l);
        out.push(head);
        buf = tail;
    }
    out
}

/// A `Vec<T>` of length `n` whose elements are left uninitialised, to be filled
/// before any read. Used only for the mesh output buffers, whose every element
/// is written exactly once by the parallel fill (counts are exact). Skips the
/// ~zeroing of tens of MB that we would immediately overwrite.
///
/// SAFETY of each call site: `T` is `Copy` plain-data (f32 vectors, Srgba bytes,
/// u32) with no `Drop` and no invalid bit patterns, and the fill writes all `n`
/// elements before the buffer is read.
fn uninit_vec<T: Copy>(n: usize) -> Vec<T> {
    let mut v = Vec::with_capacity(n);
    // SAFETY: see the doc comment — every element is written before it is read,
    // and T has no invalid bit patterns and no Drop glue.
    unsafe {
        v.set_len(n);
    }
    v
}

/// Group items into ≤ `n_chunks` *contiguous* `[start, end)` ranges of roughly
/// equal total weight. Balancing by work (e.g. segment count) instead of item
/// count keeps the slowest thread from being one chunk that happened to hold
/// several big trees. Contiguity preserves output order → bit-identical.
fn balanced_ranges(weights: &[usize], n_chunks: usize) -> Vec<(usize, usize)> {
    let total: usize = weights.iter().sum();
    let target = (total / n_chunks.max(1)).max(1);
    let mut ranges = Vec::with_capacity(n_chunks);
    let (mut start, mut acc) = (0usize, 0usize);
    for (i, &w) in weights.iter().enumerate() {
        acc += w;
        // Close the chunk once it reaches the target (but leave room for the
        // remaining items to each get a chunk, so we never exceed n_chunks).
        if acc >= target && ranges.len() + 1 < n_chunks && weights.len() - (i + 1) >= n_chunks - (ranges.len() + 1) {
            ranges.push((start, i + 1));
            start = i + 1;
            acc = 0;
        }
    }
    if start < weights.len() || ranges.is_empty() {
        ranges.push((start, weights.len()));
    }
    ranges
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

/// Per-plant foliage appearance, derived from its genome (see
/// `Genome::leaf_rgb`/`foliage_style`). `rgb` is the leaf colour; `needle` ∈
/// [0,1] morphs the leaf blade from a broad diamond (0 — broadleaf) to a long
/// thin needle pointing along the twig (1 — conifer spray), so a tall-narrow
/// (excurrent, conifer-like) genome reads as needleleaf and a short-broad one as
/// broadleaf — the same slenderness axis that already sets leaf hue.
#[derive(Clone, Copy)]
pub struct LeafStyle {
    pub rgb: [u8; 3],
    pub needle: f32,
}

/// The four corners (+ normal) of one leaf blade, given its attachment point,
/// the radial spray direction, the twig direction, the (randomised) size, and
/// the broad↔needle factor. Broad: a wide diamond fanning outward; needle: a
/// longer, much thinner blade angled more along the twig. Shared by the forest
/// and single-plant builders so the two stay identical.
fn leaf_blade(
    base_pt: GVec3,
    radial: GVec3,
    dir: GVec3,
    size: f32,
    needle: f32,
) -> ([GVec3; 4], GVec3) {
    // Needles point more along the twig, run longer, and are far narrower.
    let along = 0.5 + 0.6 * needle;
    let len = size * (1.0 + 0.7 * needle);
    let wfrac = 0.38 - 0.28 * needle;
    let leaf_dir = (radial + dir * along).normalize_or_zero();
    let width_axis = leaf_dir.cross(radial).normalize_or_zero();
    let tip = base_pt + leaf_dir * len;
    let mid = base_pt + leaf_dir * (len * 0.45);
    let half_w = width_axis * (len * wfrac);
    let p0 = base_pt;
    let p1 = mid + half_w;
    let p2 = tip;
    let p3 = mid - half_w;
    let normal = (p1 - p0).cross(p3 - p0).normalize_or_zero();
    ([p0, p1, p2, p3], normal)
}

/// Build a foliage mesh: a small fan of leaf blades at each twig point, fanning
/// outward around the twig direction. Per-leaf green variation via vertex
/// colors. `points` is (position, outward direction); `needle` ∈ [0,1] morphs
/// broad↔needle (see `LeafStyle`). Colour comes from the material albedo (the
/// single-plant viewer sets it per species), so the vertex colours here are
/// only near-white brightness variation.
pub fn build_foliage_mesh(
    points: &[(GVec3, GVec3)],
    leaf_size: f32,
    per_cluster: usize,
    needle: f32,
) -> CpuMesh {
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
            let size = leaf_size * (0.7 + 0.6 * hash01(seed.wrapping_add(7)));
            let base_pt = *pos + dir * (leaf_size * 0.2);
            let ([p0, p1, p2, p3], normal) = leaf_blade(base_pt, radial, dir, size, needle);
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

/// Fill one chunk of trunk batches directly into pre-sized output slices.
/// `vbase` is the chunk's first global vertex index, so emitted indices are
/// absolute. The slice lengths exactly match the work (no degenerate-segment
/// skip: `skeleton()` only emits segments of length ≥ 1e-5).
#[allow(clippy::too_many_arguments)]
fn fill_trunk(
    batches: &[(Vec<Segment>, [u8; 3])],
    sides: usize,
    vbase: u32,
    pos: &mut [Vector3<f32>],
    nrm: &mut [Vector3<f32>],
    col: &mut [Srgba],
    idx: &mut [u32],
) {
    let mut vi = 0usize; // local vertex cursor
    let mut ii = 0usize; // local index cursor
    for (segs, rgb) in batches {
        let c = Srgba::new(rgb[0], rgb[1], rgb[2], 255);
        for s in segs {
            let axis = s.b - s.a;
            let len = axis.length();
            let dir = if len < 1e-6 { GVec3::Y } else { axis / len };
            let (u, v) = dir.any_orthonormal_pair();
            let dr = s.rb - s.ra;
            let local_base = vi as u32;
            for k in 0..sides {
                let ang = std::f32::consts::TAU * (k as f32) / (sides as f32);
                let radial = (u * ang.cos() + v * ang.sin()).normalize_or_zero();
                let normal = (radial * len - dir * dr).normalize_or_zero();
                pos[vi] = g2t(s.a + radial * s.ra);
                nrm[vi] = g2t(normal);
                col[vi] = c;
                vi += 1;
                pos[vi] = g2t(s.b + radial * s.rb);
                nrm[vi] = g2t(normal);
                col[vi] = c;
                vi += 1;
            }
            for k in 0..sides {
                let a0 = vbase + local_base + 2 * k as u32;
                let b0 = a0 + 1;
                let kn = (k + 1) % sides;
                let a1 = vbase + local_base + 2 * kn as u32;
                let b1 = a1 + 1;
                idx[ii..ii + 6].copy_from_slice(&[a0, b0, b1, a0, b1, a1]);
                ii += 6;
            }
        }
    }
}

/// Forest trunk mesh: one combined mesh over many plants, each batch of
/// segments tinted with that plant's bark colour via vertex colours (rendered
/// against a white material). Built in parallel: each batch chunk writes
/// directly into its disjoint slice of the final buffers (offsets from a
/// prefix-sum of exact per-chunk vertex counts), so there is neither per-thread
/// allocation nor a concat copy. Bit-identical to a sequential build.
pub fn build_forest_mesh(batches: &[(Vec<Segment>, [u8; 3])], sides: usize) -> CpuMesh {
    let sides = sides.max(3);
    let n_chunks = n_mesh_chunks(batches.len());
    // Balance chunks by segment count (work), keeping them contiguous.
    let weights: Vec<usize> = batches.iter().map(|(s, _)| s.len()).collect();
    let chunks: Vec<&[(Vec<Segment>, [u8; 3])]> =
        balanced_ranges(&weights, n_chunks).iter().map(|&(s, e)| &batches[s..e]).collect();
    // Exact per-chunk vertex / index counts (sides*2 verts, sides*6 indices per
    // segment) and the running vertex offset of each chunk.
    let vc: Vec<usize> =
        chunks.iter().map(|c| c.iter().map(|(s, _)| s.len()).sum::<usize>() * sides * 2).collect();
    let ic: Vec<usize> = vc.iter().map(|&v| v / 2 * 6).collect();
    let mut voff = vec![0u32; chunks.len()];
    let mut acc = 0u32;
    for (i, &v) in vc.iter().enumerate() {
        voff[i] = acc;
        acc += v as u32;
    }
    let total_v = vc.iter().sum::<usize>();
    let total_i = ic.iter().sum::<usize>();

    let mut positions = uninit_vec::<Vector3<f32>>(total_v);
    let mut normals = uninit_vec::<Vector3<f32>>(total_v);
    let mut colors = uninit_vec::<Srgba>(total_v);
    let mut indices = uninit_vec::<u32>(total_i);
    let ps = carve_mut(&mut positions, &vc);
    let ns = carve_mut(&mut normals, &vc);
    let cs = carve_mut(&mut colors, &vc);
    let is = carve_mut(&mut indices, &ic);

    std::thread::scope(|scope| {
        for (((((chunk, &vb), p), n), c), i) in chunks
            .iter()
            .zip(&voff)
            .zip(ps)
            .zip(ns)
            .zip(cs)
            .zip(is)
        {
            let chunk = *chunk;
            scope.spawn(move || fill_trunk(chunk, sides, vb, p, n, c, i));
        }
    });

    CpuMesh {
        positions: Positions::F32(positions),
        indices: Indices::U32(indices),
        normals: Some(normals),
        colors: Some(colors),
        ..Default::default()
    }
}

/// Fill one chunk of foliage batches directly into pre-sized output slices.
/// `base_bi` is the chunk's first global batch index (the per-leaf hash seed
/// uses it, so threading it keeps the result bit-identical); `vbase` is the
/// chunk's first global vertex index, so emitted indices are absolute.
#[allow(clippy::too_many_arguments)]
fn fill_foliage(
    batches: &[(Vec<(GVec3, GVec3)>, LeafStyle)],
    base_bi: usize,
    vbase: u32,
    leaf_size: f32,
    per_cluster: usize,
    pos: &mut [Vector3<f32>],
    nrm: &mut [Vector3<f32>],
    col: &mut [Srgba],
    idx: &mut [u32],
) {
    let mut vi = 0usize;
    let mut ii = 0usize;
    for (local_bi, (points, style)) in batches.iter().enumerate() {
        let bi = base_bi + local_bi;
        let rgb = style.rgb;
        for (ti, (pos_p, dir)) in points.iter().enumerate() {
            let dir = dir.normalize_or_zero();
            let (u, v) = if dir.length_squared() > 1e-6 {
                dir.any_orthonormal_pair()
            } else {
                (GVec3::X, GVec3::Z)
            };
            for j in 0..per_cluster {
                let seed = (bi as u32)
                    .wrapping_mul(1009)
                    .wrapping_add((ti as u32).wrapping_mul(97))
                    .wrapping_add(j as u32);
                let az =
                    std::f32::consts::TAU * (j as f32 / per_cluster as f32) + hash01(seed) * 1.2;
                let radial = (u * az.cos() + v * az.sin()).normalize_or_zero();
                let size = leaf_size * (0.7 + 0.6 * hash01(seed.wrapping_add(7)));
                let base_pt = *pos_p + dir * (leaf_size * 0.2);
                let ([p0, p1, p2, p3], normal) =
                    leaf_blade(base_pt, radial, dir, size, style.needle);

                // Per-leaf brightness around the species leaf colour.
                let t = 0.75 + 0.4 * hash01(seed.wrapping_add(31));
                let c = Srgba::new(
                    (rgb[0] as f32 * t).min(255.0) as u8,
                    (rgb[1] as f32 * t).min(255.0) as u8,
                    (rgb[2] as f32 * t).min(255.0) as u8,
                    255,
                );
                let base_idx = vbase + vi as u32;
                let nrm_t = g2t(normal);
                for p in [p0, p1, p2, p3] {
                    pos[vi] = g2t(p);
                    nrm[vi] = nrm_t;
                    col[vi] = c;
                    vi += 1;
                }
                idx[ii..ii + 6].copy_from_slice(&[
                    base_idx,
                    base_idx + 1,
                    base_idx + 2,
                    base_idx,
                    base_idx + 2,
                    base_idx + 3,
                ]);
                ii += 6;
            }
        }
    }
}

/// Forest foliage mesh: leaf-quad fans over many plants, each batch tinted with
/// that plant's leaf colour (plus per-leaf brightness variation). Built in
/// parallel like the trunk mesh — each chunk fills its disjoint slice (exact
/// counts: every twig yields per_cluster·4 verts) — bit-identical to sequential.
pub fn build_forest_foliage(
    batches: &[(Vec<(GVec3, GVec3)>, LeafStyle)],
    leaf_size: f32,
    per_cluster: usize,
) -> CpuMesh {
    let n_chunks = n_mesh_chunks(batches.len());
    // Balance chunks by twig count (work), keeping them contiguous.
    let weights: Vec<usize> = batches.iter().map(|(p, _)| p.len()).collect();
    let chunks: Vec<&[(Vec<(GVec3, GVec3)>, LeafStyle)]> =
        balanced_ranges(&weights, n_chunks).iter().map(|&(s, e)| &batches[s..e]).collect();
    // Exact per-chunk counts (per_cluster*4 verts / per_cluster*6 indices per
    // twig), the chunk's first global batch index, and its vertex offset.
    let vc: Vec<usize> = chunks
        .iter()
        .map(|c| c.iter().map(|(p, _)| p.len()).sum::<usize>() * per_cluster * 4)
        .collect();
    let ic: Vec<usize> = vc.iter().map(|&v| v / 4 * 6).collect();
    let mut voff = vec![0u32; chunks.len()];
    let mut boff = vec![0usize; chunks.len()];
    let (mut vacc, mut bacc) = (0u32, 0usize);
    for (i, (&v, chunk)) in vc.iter().zip(&chunks).enumerate() {
        voff[i] = vacc;
        boff[i] = bacc;
        vacc += v as u32;
        bacc += chunk.len();
    }
    let total_v = vc.iter().sum::<usize>();
    let total_i = ic.iter().sum::<usize>();

    let mut positions = uninit_vec::<Vector3<f32>>(total_v);
    let mut normals = uninit_vec::<Vector3<f32>>(total_v);
    let mut colors = uninit_vec::<Srgba>(total_v);
    let mut indices = uninit_vec::<u32>(total_i);
    let ps = carve_mut(&mut positions, &vc);
    let ns = carve_mut(&mut normals, &vc);
    let cs = carve_mut(&mut colors, &vc);
    let is = carve_mut(&mut indices, &ic);

    std::thread::scope(|scope| {
        for ((((((chunk, &vb), &bb), p), n), c), i) in chunks
            .iter()
            .zip(&voff)
            .zip(&boff)
            .zip(ps)
            .zip(ns)
            .zip(cs)
            .zip(is)
        {
            let chunk = *chunk;
            scope.spawn(move || {
                fill_foliage(chunk, bb, vb, leaf_size, per_cluster, p, n, c, i)
            });
        }
    });

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
