//! Branch module prototypes (Sec. 5.1).
//!
//! A prototype is the *topology* of a branching structure: a connected acyclic
//! graph with a single root node and a set of terminal nodes that serve as
//! connectors for child modules. The paper uses ~9 prototypes positioned in a
//! "morphospace" spanned by apical control (lambda) and determinacy (D); a new
//! module picks the Voronoi-nearest prototype to its query point.
//!
//! Node positions here are the *mature local layout* in module space (we grow
//! the plant "up", i.e. the module axis is roughly +Y). They encode segment
//! directions and rest-lengths only; actual lengths/diameters come from the
//! development equations (Sec. 5.3).

use glam::Vec3;

#[derive(Clone, Debug)]
pub struct ProtoNode {
    /// Mature local position in module space.
    pub pos: Vec3,
    /// Parent node index (None for the root, which is node 0).
    pub parent: Option<usize>,
    /// Topological depth from the root (root = 0). Used for intra-module
    /// acropetal development timing (segments closer to the root grow first).
    pub depth: u32,
}

#[derive(Clone, Debug)]
pub struct Prototype {
    pub name: &'static str,
    /// nodes[0] is always the root (parent == None).
    pub nodes: Vec<ProtoNode>,
    /// Terminal node indices. By convention `terminals[0]` is the *apical*
    /// terminal (continuation of the main axis), which receives the apical
    /// share of vigor in the Borchert-Honda redistribution.
    pub terminals: Vec<usize>,
    /// Morphospace coordinate on the apical-control axis.
    pub lambda: f32,
    /// Morphospace coordinate on the determinacy axis.
    pub determinacy: f32,
}

impl Prototype {
    /// Normalized local direction of the segment ending at `child`.
    pub fn seg_dir_local(&self, child: usize) -> Vec3 {
        let n = &self.nodes[child];
        let p = n.parent.expect("segment child must have a parent");
        (self.nodes[child].pos - self.nodes[p].pos).normalize_or_zero()
    }

    /// Mature (rest) length of the segment ending at `child`.
    pub fn seg_restlen(&self, child: usize) -> f32 {
        let n = &self.nodes[child];
        let p = n.parent.expect("segment child must have a parent");
        (self.nodes[child].pos - self.nodes[p].pos).length()
    }

    pub fn is_terminal(&self, node: usize) -> bool {
        self.terminals.contains(&node)
    }

    /// Centroid of the mature local node layout (module-local frame). Used to
    /// place the module's predicted bounding sphere during orientation
    /// optimization.
    pub fn local_centroid(&self) -> Vec3 {
        let sum: Vec3 = self.nodes.iter().map(|n| n.pos).fold(Vec3::ZERO, |a, b| a + b);
        sum / self.nodes.len() as f32
    }

    /// Enclosing radius of the mature local layout about its centroid.
    pub fn local_radius(&self) -> f32 {
        let c = self.local_centroid();
        self.nodes
            .iter()
            .map(|n| (n.pos - c).length())
            .fold(0.0f32, f32::max)
            + 0.1
    }
}

/// Helper to build a prototype from a flat list of (position, parent) pairs and
/// an explicit terminal list. Depths are derived from the parent chain.
fn build(
    name: &'static str,
    lambda: f32,
    determinacy: f32,
    raw: &[(Vec3, Option<usize>)],
    terminals: Vec<usize>,
) -> Prototype {
    let mut nodes: Vec<ProtoNode> = raw
        .iter()
        .map(|(pos, parent)| ProtoNode {
            pos: *pos,
            parent: *parent,
            depth: 0,
        })
        .collect();
    // Derive depths (raw list is assumed parent-before-child, which all the
    // definitions below respect).
    for i in 0..nodes.len() {
        nodes[i].depth = match nodes[i].parent {
            None => 0,
            Some(p) => nodes[p].depth + 1,
        };
    }
    Prototype {
        name,
        nodes,
        terminals,
        lambda,
        determinacy,
    }
}

/// Parametric module prototype. Geometry is derived from the morphospace
/// coordinates so the nine prototypes vary continuously:
///   * apical control `lambda` sets the branching angle (high λ → narrow,
///     excurrent; low λ → wide, decurrent);
///   * determinacy `d` sets the topology (high D → monopodial: a straight
///     apical continuation plus laterals; low D → sympodial: equal slanted
///     forks with no dominant axis).
/// Laterals are spread in azimuth for a 3D crown.
fn make_proto(name: &'static str, lambda: f32, d: f32) -> Prototype {
    use glam::vec3;
    use std::f32::consts::{PI, TAU};

    let internode = 0.8;
    let blen = 0.85;
    let ba = (25.0 + (1.0 - lambda) * 37.0).to_radians(); // 25°..62°
    let top = vec3(0.0, internode, 0.0);
    let lateral = |phi: f32| -> Vec3 {
        top + vec3(ba.sin() * phi.cos(), ba.cos(), ba.sin() * phi.sin()) * blen
    };

    let mut raw: Vec<(Vec3, Option<usize>)> = vec![
        (vec3(0.0, 0.0, 0.0), None),  // 0 root
        (top, Some(0)),               // 1 internode top
    ];
    let mut terminals = Vec::new();

    if d >= 0.45 {
        // Monopodial: straight apical continuation + two opposed laterals.
        raw.push((top + vec3(0.04, 0.9, 0.0), Some(1)));
        terminals.push(raw.len() - 1); // apical first
        for phi in [0.0, PI] {
            raw.push((lateral(phi), Some(1)));
            terminals.push(raw.len() - 1);
        }
    } else {
        // Sympodial: equal slanted forks, no dominant axis (more forks when D
        // is very low). terminals[0] is nominally apical but is itself slanted.
        let n = if d < 0.3 { 3 } else { 2 };
        for k in 0..n {
            let phi = TAU * k as f32 / n as f32;
            raw.push((lateral(phi), Some(1)));
            terminals.push(raw.len() - 1);
        }
    }

    build(name, lambda, d, &raw, terminals)
}

/// The default prototype library: nine prototypes on a (λ, D) grid spanning the
/// morphospace. A vigorous parent (high D′) selects monopodial modules; a weak
/// one selects sympodial — giving intra-tree variation as well as species
/// variation via the plant's λ.
pub fn default_library() -> Vec<Prototype> {
    const NAMES: [&str; 9] = [
        "sympodial-wide", "forked-wide", "monopodial-wide",
        "sympodial-mid", "forked-mid", "monopodial-mid",
        "sympodial-narrow", "forked-narrow", "monopodial-narrow",
    ];
    let lambdas = [0.25f32, 0.55, 0.85];
    let ds = [0.25f32, 0.55, 0.85];
    let mut out = Vec::with_capacity(9);
    for (li, &lam) in lambdas.iter().enumerate() {
        for (di, &dd) in ds.iter().enumerate() {
            out.push(make_proto(NAMES[li * 3 + di], lam, dd));
        }
    }
    out
}
