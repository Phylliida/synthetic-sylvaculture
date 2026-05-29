//! Plant architecture and growth simulation (Secs. 5.2, 5.3).
//!
//! A plant is an ordered tree of branch-module instances (the *module
//! architecture*) with a single root module. Each simulation step:
//!   1. basipetal light pass:   Q(u) = exp(-collisions), accumulate to root
//!   2. acropetal vigor pass:   redistribute v̄ with apical control λ (Eq. 2)
//!   3. development:            growth rate ϒ (Eq. 5), integrate age (Eq. 6)
//!   4. structural change:      attach new modules at mature terminals, shed
//!                              modules whose vigor fell below v̄min
//!
//! Milestone 1 keeps light uniform (Q=1, no collisions yet — that arrives with
//! orientation optimization) so the visible behavior is driven purely by the
//! vigor redistribution and the development/geometry equations.

use crate::prototype::Prototype;
use glam::{Quat, Vec3};
use std::collections::HashMap;

pub type ModuleId = usize;

/// A render-ready truncated-cone branch segment in world space.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub a: Vec3,
    pub b: Vec3,
    pub ra: f32,
    pub rb: f32,
}

/// Per plant-type structural parameters. Names follow the paper / Tab. 4.
#[derive(Clone, Debug)]
pub struct PlantParams {
    /// Maximum vigor allocated to the root module (v̄rootmax).
    pub v_root_max: f32,
    /// Plant-scale growth rate (g_p), scales every module's growth rate.
    pub gp: f32,
    /// Apical control λ (Eq. 2). >0.5 excurrent, ≤0.5 decurrent.
    pub lambda: f32,
    /// Determinacy D (morphospace query axis). High D -> monopodial.
    pub determinacy: f32,
    /// Shedding / attachment threshold (v̄min): modules below it are shed.
    pub v_min: f32,
    /// Normalization for the growth-rate sigmoid (v̄max in Eq. 5). We use
    /// v_root_max.
    pub v_max: f32,
    /// Module physiological age at which it is fully developed (a_mature).
    pub a_mature: f32,
    /// Branch length scaling coefficient (β, Eq. 9).
    pub beta: f32,
    /// Maximum branch-segment length (ℓmax, Eq. 9).
    pub l_max: f32,
    /// Thickening factor / minimum segment diameter (φ, Eq. 8).
    pub phi: f32,
    /// Tropism temporal decay (g1, Eq. 10).
    pub g1: f32,
    /// Tropism strength/sign (g2, Eq. 10). Negative = gravitropism (droop),
    /// positive = phototropism. Not tabulated in the paper's Tab. 4; chosen
    /// here pending the Palubicki 2009 details.
    pub g2: f32,
    /// Maximum plant age before senescence begins (p_max).
    pub p_max: f32,
}

impl Default for PlantParams {
    fn default() -> Self {
        Self {
            v_root_max: 100.0,
            gp: 0.35,
            lambda: 0.72,
            determinacy: 0.8,
            v_min: 1.0,
            v_max: 100.0,
            a_mature: 1.0,
            beta: 1.0,
            l_max: 1.2,
            phi: 0.02,
            g1: 1.0,
            g2: -0.35,
            p_max: 1.0e9, // effectively no senescence in milestone 1
        }
    }
}

/// One instantiated branch module.
#[derive(Clone, Debug)]
pub struct Module {
    pub proto: usize,
    pub parent: Option<ModuleId>,
    /// Absolute orientation of the module frame in world space.
    pub orientation: Quat,
    /// World position of this module's root node (the attachment point).
    pub base_pos: Vec3,
    /// Children, paired with the terminal node index (in *this* module's
    /// prototype) they attach to. `children[*].0 == terminals[0]` is apical.
    pub children: Vec<(usize, ModuleId)>,
    /// Physiological age a_u (Eq. 6), in [0, a_mature].
    pub age: f32,
    /// Plant-scale vigor v̄(u) (Eq. 2).
    pub vigor: f32,
    /// Local light exposure Q(u) = exp(-collisions). Uniform (=1) for now.
    pub q_local: f32,
    /// Accumulated light flux through this module's subtree (basipetal pass).
    pub q_acc: f32,
}

impl Module {
    fn new(proto: usize, parent: Option<ModuleId>, orientation: Quat, base_pos: Vec3) -> Self {
        Self {
            proto,
            parent,
            orientation,
            base_pos,
            children: Vec::new(),
            age: 0.0,
            vigor: 0.0,
            q_local: 1.0,
            q_acc: 1.0,
        }
    }

    pub fn is_mature(&self, p: &PlantParams) -> bool {
        self.age >= p.a_mature
    }
}

pub struct Plant {
    pub protos: Vec<Prototype>,
    /// Module storage. `None` slots are shed modules (indices stay stable).
    pub modules: Vec<Option<Module>>,
    pub root: ModuleId,
    pub params: PlantParams,
    /// Plant age p_t (in simulation steps / "years").
    pub age: f32,
    pub origin: Vec3,
}

impl Plant {
    pub fn new(protos: Vec<Prototype>, params: PlantParams, origin: Vec3) -> Self {
        // Root module grows straight up.
        let root_mod = Module::new(0, None, Quat::IDENTITY, origin);
        Plant {
            protos,
            modules: vec![Some(root_mod)],
            root: 0,
            params,
            age: 0.0,
            origin,
        }
    }

    pub fn module(&self, id: ModuleId) -> &Module {
        self.modules[id].as_ref().unwrap()
    }
    fn module_mut(&mut self, id: ModuleId) -> &mut Module {
        self.modules[id].as_mut().unwrap()
    }
    pub fn alive_ids(&self) -> Vec<ModuleId> {
        (0..self.modules.len())
            .filter(|&i| self.modules[i].is_some())
            .collect()
    }
    pub fn module_count(&self) -> usize {
        self.modules.iter().filter(|m| m.is_some()).count()
    }

    /// Advance the simulation by one step of size `dt`.
    pub fn step(&mut self, dt: f32) {
        self.age += dt;
        self.light_pass();
        self.vigor_pass();
        self.develop(dt);
        self.shed();
    }

    // --- 1. basipetal light accumulation ------------------------------------
    // Q(u) = Q(u_m) + Q(u_l): a module's accumulated flux is its own local light
    // plus the sum of its children's accumulated flux. Post-order over the tree.
    fn light_pass(&mut self) {
        let order = self.post_order(self.root);
        for &id in &order {
            let m = self.module(id);
            let mut acc = m.q_local;
            let kids: Vec<ModuleId> = m.children.iter().map(|(_, c)| *c).collect();
            for c in kids {
                acc += self.module(c).q_acc;
            }
            self.module_mut(id).q_acc = acc;
        }
    }

    // --- 2. acropetal vigor redistribution (Eq. 2) --------------------------
    fn vigor_pass(&mut self) {
        let p = self.params.clone();
        // Senescence: once past p_max, linearly ramp the root allotment to 0.
        let senescence = if self.age <= p.p_max {
            1.0
        } else {
            (1.0 - (self.age - p.p_max) / p.p_max.max(1.0)).clamp(0.0, 1.0)
        };
        let v_root = p.v_root_max * senescence;

        // Pre-order: parent's vigor is known before its children's.
        let order = self.pre_order(self.root);
        for &id in &order {
            if id == self.root {
                self.module_mut(id).vigor = v_root;
            }
            let v_u = self.module(id).vigor;
            let children: Vec<(usize, ModuleId)> = self.module(id).children.clone();
            if children.is_empty() {
                continue;
            }
            // Borchert-Honda split generalized to >2 children:
            //   weight = λ for the apical child, (1-λ) for each lateral,
            //   share_i = v_u * w_i Q_i / Σ_j w_j Q_j.
            // The apical child is the one attached to terminals[0].
            let apical_terminal = self.protos[self.module(id).proto].terminals[0];
            let mut denom = 0.0f32;
            let mut weights: Vec<f32> = Vec::with_capacity(children.len());
            for (term, cid) in &children {
                let w = if *term == apical_terminal {
                    p.lambda
                } else {
                    1.0 - p.lambda
                };
                let q = self.module(*cid).q_acc;
                let wq = w * q;
                weights.push(wq);
                denom += wq;
            }
            for (i, (_term, cid)) in children.iter().enumerate() {
                let share = if denom > 1e-9 {
                    v_u * weights[i] / denom
                } else {
                    v_u / children.len() as f32
                };
                self.module_mut(*cid).vigor = share;
            }
        }
    }

    // --- 3. development: grow ages, attach new modules ----------------------
    fn develop(&mut self, dt: f32) {
        let p = self.params.clone();
        let ids = self.alive_ids();

        // Integrate physiological age for every module (Eqs. 5, 6).
        for &id in &ids {
            let v = self.module(id).vigor;
            let x = ((v - p.v_min) / (p.v_max - p.v_min)).clamp(0.0, 1.0);
            let s = 3.0 * x * x - 2.0 * x * x * x; // sigmoid S(x)
            let growth_rate = s * p.gp;
            let m = self.module_mut(id);
            m.age = (m.age + growth_rate * dt).min(p.a_mature);
        }

        // Attach new modules at mature, unoccupied terminals.
        for &id in &ids {
            if !self.module(id).is_mature(&p) {
                continue;
            }
            let proto_idx = self.module(id).proto;
            let v_u = self.module(id).vigor;
            let terminals = self.protos[proto_idx].terminals.clone();
            let apical = terminals[0];
            let occupied: Vec<usize> =
                self.module(id).children.iter().map(|(t, _)| *t).collect();

            for &term in &terminals {
                if occupied.contains(&term) {
                    continue;
                }
                // Estimate the vigor that would flow to this terminal.
                let w = if term == apical {
                    p.lambda
                } else {
                    1.0 - p.lambda
                };
                let term_vigor = v_u * w;
                if term_vigor <= p.v_min {
                    continue;
                }
                self.attach_child(id, term, term_vigor);
            }
        }
    }

    /// Create and attach a new module at terminal `term` of module `parent_id`.
    /// `init_vigor` seeds the new module's vigor so it survives the shedding
    /// pass in the step it is born (the next vigor pass recomputes it exactly).
    fn attach_child(&mut self, parent_id: ModuleId, term: usize, init_vigor: f32) {
        let parent = self.module(parent_id);
        let parent_proto = &self.protos[parent.proto];

        // World position & direction of the parent terminal node.
        let term_local_pos = parent_proto.nodes[term].pos;
        let term_local_dir = parent_proto.seg_dir_local(term);
        let base_pos = parent.base_pos + parent.orientation * term_local_pos;
        let world_dir = (parent.orientation * term_local_dir).normalize_or_zero();

        // Select prototype via morphospace (Voronoi-nearest to (λ, D')).
        let parent_vigor = parent.vigor;
        let d_prime = parent_vigor * self.params.determinacy / self.params.v_max;
        let proto_idx = self.select_prototype(self.params.lambda, d_prime);

        // Orient the child so its local axis (+Y) aligns with the terminal's
        // world direction.
        let orientation = Quat::from_rotation_arc(Vec3::Y, world_dir);

        let mut child = Module::new(proto_idx, Some(parent_id), orientation, base_pos);
        child.vigor = init_vigor;
        let new_id = self.alloc(child);
        self.module_mut(parent_id).children.push((term, new_id));
    }

    /// Morphospace selection: nearest prototype to the query point in (λ, D).
    fn select_prototype(&self, lambda: f32, d: f32) -> usize {
        let mut best = 0;
        let mut best_d2 = f32::INFINITY;
        for (i, proto) in self.protos.iter().enumerate() {
            let dl = proto.lambda - lambda;
            let dd = proto.determinacy - d;
            let d2 = dl * dl + dd * dd;
            if d2 < best_d2 {
                best_d2 = d2;
                best = i;
            }
        }
        best
    }

    // --- 4. shedding --------------------------------------------------------
    fn shed(&mut self) {
        let p = self.params.clone();
        // A module is shed if its vigor dropped below v̄min (but never the root
        // unless senescence has fully drained it).
        let to_shed: Vec<ModuleId> = self
            .alive_ids()
            .into_iter()
            .filter(|&id| id != self.root && self.module(id).vigor < p.v_min)
            .collect();
        for id in to_shed {
            // It may already have been removed as part of an ancestor's subtree.
            if self.modules[id].is_some() {
                self.remove_subtree(id);
            }
        }
    }

    fn remove_subtree(&mut self, id: ModuleId) {
        // Detach from parent's child list.
        if let Some(parent) = self.module(id).parent {
            if let Some(pm) = self.modules[parent].as_mut() {
                pm.children.retain(|(_, c)| *c != id);
            }
        }
        // Remove descendants (collect first to avoid borrow issues).
        let mut stack = vec![id];
        let mut dead = Vec::new();
        while let Some(cur) = stack.pop() {
            if let Some(m) = self.modules[cur].as_ref() {
                for (_, c) in &m.children {
                    stack.push(*c);
                }
                dead.push(cur);
            }
        }
        for d in dead {
            self.modules[d] = None;
        }
    }

    // --- storage helpers ----------------------------------------------------
    fn alloc(&mut self, m: Module) -> ModuleId {
        if let Some(slot) = self.modules.iter().position(|s| s.is_none()) {
            self.modules[slot] = Some(m);
            slot
        } else {
            self.modules.push(Some(m));
            self.modules.len() - 1
        }
    }

    fn pre_order(&self, root: ModuleId) -> Vec<ModuleId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            out.push(id);
            for (_, c) in &self.module(id).children {
                stack.push(*c);
            }
        }
        out
    }

    fn post_order(&self, root: ModuleId) -> Vec<ModuleId> {
        let mut pre = self.pre_order(root);
        pre.reverse();
        pre
    }

    /// Total physiological "height" reached: max world-Y over the skeleton.
    pub fn height(&self) -> f32 {
        self.skeleton()
            .iter()
            .map(|s| s.b.y.max(s.a.y))
            .fold(0.0, f32::max)
    }

    // --- geometry: derive a render skeleton ---------------------------------
    //
    // Build a single global node graph spanning all modules (a child module's
    // root node is the *same* point as the parent terminal it attaches to, so
    // there is one continuous skeleton). For each node we compute:
    //   * world position: parent + orientation·dir·ℓ + tropism offset, where
    //     ℓ = min(ℓmax, β·a_b) (Eq. 9) and a_b = max(0, a_u − a_n) (Eq. 7) gives
    //     acropetal (root-first) extension within the module;
    //   * tropism offset τ(a_b) = (g1·g2 / (a_b + g1))·ĝ (Eq. 10), bending young
    //     segments and leaving old ones set;
    //   * diameter via the Pipe Model d = √Σ d_child² (Eq. 8), φ at the tips.
    //
    // Because modules are visited parent-before-child and nodes parent-before-
    // child, every node's parent has a smaller global id — so a reverse-id sweep
    // is a valid post-order for the diameter accumulation.
    pub fn skeleton(&self) -> Vec<Segment> {
        let p = &self.params;
        let mut pos: Vec<Vec3> = Vec::new();
        let mut parent: Vec<Option<usize>> = Vec::new();
        let mut children: Vec<Vec<usize>> = Vec::new();
        let mut module_root_gid: HashMap<ModuleId, usize> = HashMap::new();

        // Plant root node.
        pos.push(self.module(self.root).base_pos);
        parent.push(None);
        children.push(Vec::new());
        module_root_gid.insert(self.root, 0);

        for &mid in &self.pre_order(self.root) {
            let m = self.module(mid);
            let proto = &self.protos[m.proto];
            let au = m.age;
            let max_depth = proto
                .nodes
                .iter()
                .map(|n| n.depth)
                .max()
                .unwrap_or(1)
                .max(1) as f32;

            let mut local_gid = vec![usize::MAX; proto.nodes.len()];
            local_gid[0] = module_root_gid[&mid];

            for ln in 1..proto.nodes.len() {
                let base_ln = proto.nodes[ln].parent.unwrap();
                let base_gid = local_gid[base_ln];

                // Acropetal timing: deeper base nodes start later (Eq. 7).
                let a_n = (proto.nodes[base_ln].depth as f32 / max_depth) * p.a_mature;
                let a_b = (au - a_n).max(0.0);
                let len = (p.beta * a_b).min(p.l_max);

                let dir_world = (m.orientation * proto.seg_dir_local(ln)).normalize_or_zero();
                let tropism = if a_b > 1e-6 {
                    (p.g1 * p.g2 / (a_b + p.g1)) * Vec3::NEG_Y
                } else {
                    Vec3::ZERO
                };
                let node_pos = pos[base_gid] + dir_world * len + tropism;

                let gid = pos.len();
                pos.push(node_pos);
                parent.push(Some(base_gid));
                children.push(Vec::new());
                children[base_gid].push(gid);
                local_gid[ln] = gid;
            }

            // A child module's root node *is* the terminal node it hangs from.
            for (term, cid) in &m.children {
                module_root_gid.insert(*cid, local_gid[*term]);
            }
        }

        // Pipe-Model diameters (Eq. 8), post-order via reverse global id.
        let n = pos.len();
        let mut diam = vec![p.phi; n];
        for g in (0..n).rev() {
            if !children[g].is_empty() {
                let s: f32 = children[g].iter().map(|&c| diam[c] * diam[c]).sum();
                diam[g] = s.sqrt().max(p.phi);
            }
        }

        let mut segs = Vec::new();
        for g in 0..n {
            if let Some(pg) = parent[g] {
                if (pos[g] - pos[pg]).length() < 1e-5 {
                    continue;
                }
                segs.push(Segment {
                    a: pos[pg],
                    b: pos[g],
                    ra: diam[pg] * 0.5,
                    rb: diam[g] * 0.5,
                });
            }
        }
        segs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prototype::default_library;

    fn grow(lambda: f32, steps: u32) -> Plant {
        let mut params = PlantParams::default();
        params.lambda = lambda;
        let mut plant = Plant::new(default_library(), params, Vec3::ZERO);
        for _ in 0..steps {
            plant.step(1.0);
        }
        plant
    }

    #[test]
    fn root_matures_and_branches() {
        let plant = grow(0.72, 6);
        // Root should have spawned its apical + lateral children by now.
        assert!(
            plant.module_count() > 1,
            "expected branching, got {} modules",
            plant.module_count()
        );
    }

    #[test]
    fn growth_stabilizes_below_unbounded() {
        // Vigor splitting must drive tip vigor below v_min, bounding the tree.
        let plant = grow(0.72, 200);
        let n = plant.module_count();
        assert!(n > 5, "tree too small: {n}");
        assert!(n < 100_000, "tree did not stabilize: {n}");
    }

    #[test]
    fn skeleton_is_nonempty_and_finite() {
        let plant = grow(0.72, 40);
        let segs = plant.skeleton();
        assert!(!segs.is_empty());
        for s in &segs {
            assert!(s.a.is_finite() && s.b.is_finite(), "non-finite segment");
            assert!(s.ra > 0.0 && s.rb > 0.0, "non-positive radius");
            // Pipe model: a segment is never thinner than φ at its tip.
            assert!(s.rb >= 0.5 * 0.02 - 1e-6);
        }
        assert!(plant.height() > 0.5, "tree barely grew: {}", plant.height());
    }

    #[test]
    fn excurrent_taller_than_decurrent() {
        // High apical control should yield a taller (trunk-dominated) form than
        // low apical control, for the same number of steps.
        let tall = grow(0.9, 60).height();
        let bushy = grow(0.2, 60).height();
        assert!(
            tall > bushy,
            "excurrent ({tall:.2}) should exceed decurrent ({bushy:.2})"
        );
    }

    #[test]
    fn trunk_thicker_than_twigs() {
        // Pipe Model: the basal segment must be the thickest.
        let plant = grow(0.8, 60);
        let segs = plant.skeleton();
        let max_r = segs.iter().map(|s| s.ra).fold(0.0, f32::max);
        let basal = segs
            .iter()
            .min_by(|a, b| a.a.y.total_cmp(&b.a.y))
            .unwrap();
        assert!(
            basal.ra >= max_r - 1e-4,
            "basal radius {} should be the largest {}",
            basal.ra,
            max_r
        );
    }
}
