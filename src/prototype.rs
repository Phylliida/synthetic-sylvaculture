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

/// The default prototype library. Milestone 1 ships a single monopodial module;
/// the full morphospace of nine is filled in incrementally. Even one prototype
/// already exhibits the excurrent/decurrent split governed by apical control.
pub fn default_library() -> Vec<Prototype> {
    use glam::vec3;

    // --- Monopodial module: a vertical internode that splits into one apical
    // continuation plus two lateral connectors. High lambda -> the apical child
    // dominates -> tall straight trunk (excurrent). Low lambda -> laterals win
    // -> bushy (decurrent). ---
    let monopodial = build(
        "monopodial",
        /*lambda*/ 0.75,
        /*determinacy*/ 0.8,
        &[
            (vec3(0.0, 0.0, 0.0), None),    // 0 root
            (vec3(0.0, 0.8, 0.0), Some(0)), // 1 internode top
            (vec3(0.0, 1.7, 0.0), Some(1)), // 2 apical terminal (straight up)
            (vec3(0.55, 1.35, 0.0), Some(1)), // 3 lateral terminal +x
            (vec3(-0.55, 1.35, 0.0), Some(1)), // 4 lateral terminal -x
        ],
        // apical first
        vec![2, 3, 4],
    );

    vec![monopodial]
}
