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
    /// Shade tolerance s_tol ∈ [0,1] (Sec. 6.2): the global-shadow light floor.
    /// Q_eff = lerp(s_tol, 1, Q·Q_G), so a tolerant plant (high s_tol) still
    /// receives light under shade. 0 = no global shadowing effect.
    pub shade_tolerance: f32,

    // --- Sec. 5.2.3 module orientation optimization + collision light ---
    /// Use collision-based light Q = exp(−scale·f_collisions) instead of Q=1.
    pub collision_light: bool,
    /// Scale on f_collisions inside the light exponential.
    pub light_scale: f32,
    /// Run gradient-descent orientation optimization for new modules.
    pub optimize_orientation: bool,
    /// Collision-avoidance weight ω1 in f_distribution (Eq. 3).
    pub omega1: f32,
    /// Tropism weight ω2 in f_distribution (Eq. 3 / Tab. 4).
    pub omega2: f32,
    /// Gravitropic set-point angle (rad) from vertical used by f_tropism (Eq. 4).
    pub tropism_set_angle: f32,
    /// Per-step rotation increment α used by the optimizer's candidate set
    /// P = {±α about local x, ±α about local z} (App. A.1).
    pub opt_angle: f32,
    /// Number of gradient-descent steps per optimization (~3 per the paper).
    pub opt_iters: u32,
    /// Under-relaxation factor in [0,1]: each step a module rotates only this
    /// fraction of the way toward its optimized orientation. <1 damps the
    /// simultaneous-update oscillation ("flicker") and converges to a fixed
    /// point. 1.0 = undamped (greedy, oscillates).
    pub opt_damping: f32,
    /// Hard cap (radians) on how far a module may rotate in a single step.
    pub opt_max_step: f32,
    /// Minimum current collision volume for a module to bother reorienting.
    /// Modules that aren't overlapping anything are left alone, so settled
    /// structure stops moving (removes residual flicker).
    pub opt_collision_eps: f32,
    /// Freeze settled modules: skip orientation optimization for modules that
    /// are mature or not currently colliding. This is what stops the crown
    /// flickering; disable only to study the old perpetual-relaxation behavior.
    pub opt_freeze_settled: bool,
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
            shade_tolerance: 0.0,
            // Sec. 5.2.3 features default OFF so the base-model tests stay
            // exact; the viewer and the orientation tests enable them.
            collision_light: false,
            light_scale: 0.5,
            optimize_orientation: false,
            omega1: 1.0,
            omega2: 0.35,
            tropism_set_angle: 0.0,
            opt_angle: 0.4,
            opt_iters: 6,
            opt_damping: 0.25,
            opt_max_step: 0.2,
            opt_collision_eps: 0.05,
            opt_freeze_settled: true,
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
        // Root module grows straight up; its prototype is chosen from the
        // morphospace at full vigor (D′ ≈ D).
        let root_proto = nearest_proto(&protos, params.lambda, params.determinacy);
        let root_mod = Module::new(root_proto, None, Quat::IDENTITY, origin);
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

    /// Advance the simulation by one step of size `dt` (standalone plant).
    pub fn step(&mut self, dt: f32) {
        self.step_impl(dt, None);
    }

    /// Advance with externally-supplied global shadow light Q_G per module
    /// (driven by the ecosystem's shadow grid, Sec. 6.2).
    pub fn step_shaded(&mut self, dt: f32, qg: &HashMap<ModuleId, f32>) {
        self.step_impl(dt, Some(qg));
    }

    fn step_impl(&mut self, dt: f32, qg: Option<&HashMap<ModuleId, f32>>) {
        self.age += dt;
        // Collision-based light reads the current geometry's bounding spheres.
        let spheres = if self.params.collision_light {
            self.module_spheres()
        } else {
            HashMap::new()
        };
        self.light_pass(&spheres, qg);
        self.vigor_pass();
        self.develop(dt);
        self.shed();
        if self.params.optimize_orientation {
            self.optimize_orientations();
        }
    }

    // --- 1. basipetal light accumulation ------------------------------------
    // Local light Q(u) = exp(−scale·f_collisions(u)) (Eq. 1, =1 when disabled),
    // folded with the global shadow Q_G as Q_eff = lerp(s_tol, 1, Q·Q_G)
    // (Sec. 6.2), then Q_acc(u) = Q_eff(u) + Σ Q_acc(child) tip-to-base.
    fn light_pass(&mut self, spheres: &HashMap<ModuleId, BSphere>, qg: Option<&HashMap<ModuleId, f32>>) {
        let stol = self.params.shade_tolerance;
        let scale = self.params.light_scale;
        let collision = self.params.collision_light;
        for id in self.alive_ids() {
            let q_col = if collision {
                (-scale * self.f_collisions(id, spheres)).exp()
            } else {
                1.0
            };
            let q_g = qg.and_then(|m| m.get(&id).copied()).unwrap_or(1.0);
            // lerp(stol, 1, q_col*q_g)
            self.module_mut(id).q_local = stol + (1.0 - stol) * (q_col * q_g);
        }
        // Basipetal accumulation.
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

    /// f_collisions(u): summed bounding-sphere intersection volume with all
    /// other modules, excluding structurally adjacent ones (parent / children),
    /// whose overlap is unavoidable (Eq. 1).
    fn f_collisions(&self, id: ModuleId, spheres: &HashMap<ModuleId, BSphere>) -> f32 {
        let su = match spheres.get(&id) {
            Some(s) => *s,
            None => return 0.0,
        };
        let parent = self.module(id).parent;
        let kids: Vec<ModuleId> = self.module(id).children.iter().map(|(_, c)| *c).collect();
        let mut sum = 0.0;
        for (&w, &sw) in spheres {
            if w == id || Some(w) == parent || kids.contains(&w) {
                continue;
            }
            sum += sphere_intersection_volume(su, sw);
        }
        sum
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
        // Total resource scales with the light the plant actually gathers
        // (Borchert-Honda v_base = α·Q_base, capped at v_root_max). avg_q is the
        // mean per-module effective light ∈ [s_tol, 1]; a shaded plant gathers
        // less and is suppressed — the mechanism behind understory stunting and
        // succession. With no shading (avg_q = 1) this is exactly v_root_max.
        let avg_q = self.module(self.root).q_acc / self.module_count().max(1) as f32;
        let v_root = p.v_root_max * senescence * avg_q.clamp(0.0, 1.0);

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
    /// The orientation is stored *relative to the parent*: the child's local
    /// axis (+Y) maps to the terminal's local direction. The per-step
    /// `optimize_orientations` relaxation then refines it.
    fn attach_child(&mut self, parent_id: ModuleId, term: usize, init_vigor: f32) {
        let parent = self.module(parent_id);
        let parent_proto = &self.protos[parent.proto];
        let term_local_dir = parent_proto.seg_dir_local(term);

        // Select prototype via morphospace (Voronoi-nearest to (λ, D')).
        let d_prime = parent.vigor * self.params.determinacy / self.params.v_root_max;
        let proto_idx = self.select_prototype(self.params.lambda, d_prime);

        let orientation = Quat::from_rotation_arc(Vec3::Y, term_local_dir);
        let mut child = Module::new(proto_idx, Some(parent_id), orientation, Vec3::ZERO);
        child.vigor = init_vigor;
        let new_id = self.alloc(child);
        self.module_mut(parent_id).children.push((term, new_id));
    }

    /// Intersection-volume ratio (Fig. 15a validation metric): summed pairwise
    /// non-adjacent intersection volume divided by total module sphere volume.
    pub fn intersection_ratio(&self) -> f32 {
        let spheres = self.module_spheres();
        let ids: Vec<ModuleId> = spheres.keys().copied().collect();
        let mut inter = 0.0;
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let (a, b) = (ids[i], ids[j]);
                let adj = self.module(a).parent == Some(b) || self.module(b).parent == Some(a);
                if adj {
                    continue;
                }
                inter += sphere_intersection_volume(spheres[&a], spheres[&b]);
            }
        }
        let total: f32 = spheres
            .values()
            .map(|s| 4.0 / 3.0 * std::f32::consts::PI * s.radius.powi(3))
            .sum();
        if total > 0.0 {
            inter / total
        } else {
            0.0
        }
    }

    /// Morphospace selection: nearest prototype to the query point in (λ, D).
    fn select_prototype(&self, lambda: f32, d: f32) -> usize {
        nearest_proto(&self.protos, lambda, d)
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

    /// Shape metrics about the plant's local origin (assumed at its base):
    /// `(height, crown_radius, apex_offset)` where crown_radius is the max
    /// horizontal reach of any node and apex_offset is the horizontal distance
    /// of the highest node from the trunk axis (a measure of how much the
    /// leading shoot leans/arcs over rather than rising straight).
    pub fn shape(&self) -> (f32, f32, f32) {
        let base = self.origin;
        let mut height = 0.0f32;
        let mut crown_radius = 0.0f32;
        let mut best_y = f32::MIN;
        let mut apex_offset = 0.0f32;
        for s in self.skeleton() {
            for p in [s.a, s.b] {
                let dx = p.x - base.x;
                let dz = p.z - base.z;
                let horiz = (dx * dx + dz * dz).sqrt();
                height = height.max(p.y - base.y);
                crown_radius = crown_radius.max(horiz);
                if p.y > best_y {
                    best_y = p.y;
                    apex_offset = horiz;
                }
            }
        }
        (height, crown_radius, apex_offset)
    }

    // --- geometry: derive a render skeleton ---------------------------------
    //
    // `place()` builds a single global node graph spanning all modules (a child
    // module's root node is the *same* point as the parent terminal it attaches
    // to, so there is one continuous skeleton). For each node we compute:
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
    fn place(&self) -> Placement {
        let p = &self.params;
        let mut pos: Vec<Vec3> = Vec::new();
        let mut parent: Vec<Option<usize>> = Vec::new();
        let mut children: Vec<Vec<usize>> = Vec::new();
        let mut gid_module: Vec<ModuleId> = Vec::new();
        let mut module_root_gid: HashMap<ModuleId, usize> = HashMap::new();
        // Module orientations are stored *relative to parent*; accumulate world
        // orientation parent-before-child (pre-order guarantees the ordering).
        let mut world_orient: HashMap<ModuleId, Quat> = HashMap::new();
        let mut module_base: HashMap<ModuleId, Vec3> = HashMap::new();

        // Plant root node.
        pos.push(self.module(self.root).base_pos);
        parent.push(None);
        children.push(Vec::new());
        gid_module.push(self.root);
        module_root_gid.insert(self.root, 0);

        for &mid in &self.pre_order(self.root) {
            let m = self.module(mid);
            let proto = &self.protos[m.proto];
            let au = m.age;
            let max_depth = proto.nodes.iter().map(|n| n.depth).max().unwrap_or(1).max(1) as f32;

            let w_orient = match m.parent {
                None => m.orientation,
                Some(par) => world_orient[&par] * m.orientation,
            };
            world_orient.insert(mid, w_orient);
            module_base.insert(mid, pos[module_root_gid[&mid]]);

            let mut local_gid = vec![usize::MAX; proto.nodes.len()];
            local_gid[0] = module_root_gid[&mid];

            for ln in 1..proto.nodes.len() {
                let base_ln = proto.nodes[ln].parent.unwrap();
                let base_gid = local_gid[base_ln];

                // Acropetal timing: deeper base nodes start later (Eq. 7).
                let a_n = (proto.nodes[base_ln].depth as f32 / max_depth) * p.a_mature;
                let a_b = (au - a_n).max(0.0);
                let len = (p.beta * a_b).min(p.l_max);

                let dir_world = (w_orient * proto.seg_dir_local(ln)).normalize_or_zero();
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
                gid_module.push(mid);
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

        Placement {
            pos,
            parent,
            children,
            gid_module,
            diam,
            world_orient,
            module_base,
        }
    }

    pub fn skeleton(&self) -> Vec<Segment> {
        let pl = self.place();
        let mut segs = Vec::new();
        for g in 0..pl.pos.len() {
            if let Some(pg) = pl.parent[g] {
                if (pl.pos[g] - pl.pos[pg]).length() < 1e-5 {
                    continue;
                }
                segs.push(Segment {
                    a: pl.pos[pg],
                    b: pl.pos[g],
                    ra: pl.diam[pg] * 0.5,
                    rb: pl.diam[g] * 0.5,
                });
            }
        }
        segs
    }

    /// Leaf attachment points for foliage: every thin twig node (diameter near
    /// φ) that has actually extended, returned as (world position, outward
    /// segment direction). Foliage clusters are drawn at these points.
    pub fn leaves(&self) -> Vec<(Vec3, Vec3)> {
        let pl = self.place();
        let twig = self.params.phi * 2.5;
        let mut out = Vec::new();
        for g in 0..pl.pos.len() {
            let pg = match pl.parent[g] {
                Some(pg) => pg,
                None => continue,
            };
            let seg = pl.pos[g] - pl.pos[pg];
            if pl.diam[g] <= twig && seg.length() > 0.15 {
                out.push((pl.pos[g], seg.normalize()));
            }
        }
        out
    }

    /// Spherical bounding volume B_u per module (Sec. 5.2.1): centre = centroid
    /// of the module's node geometry, radius = enclosing radius (+ small margin).
    /// Returns a map keyed by module id. Modules with no developed geometry yet
    /// are given a small sphere at their attachment point.
    pub fn module_spheres(&self) -> HashMap<ModuleId, BSphere> {
        Self::spheres_from(&self.place())
    }

    /// Centroids of fully-mature modules only — these should be frozen, so any
    /// per-step movement here is genuine flicker (not growth).
    pub fn mature_centroids(&self) -> HashMap<ModuleId, Vec3> {
        let am = self.params.a_mature;
        self.module_spheres()
            .into_iter()
            .filter(|(id, _)| self.module(*id).age >= am)
            .map(|(id, s)| (id, s.centre))
            .collect()
    }

    /// World-space centre of each module (for shadow-grid deposition).
    pub fn module_centres(&self) -> Vec<(ModuleId, Vec3)> {
        Self::spheres_from(&self.place())
            .into_iter()
            .map(|(id, s)| (id, s.centre))
            .collect()
    }

    fn spheres_from(pl: &Placement) -> HashMap<ModuleId, BSphere> {
        let mut acc: HashMap<ModuleId, Vec<Vec3>> = HashMap::new();
        for g in 0..pl.pos.len() {
            acc.entry(pl.gid_module[g]).or_default().push(pl.pos[g]);
        }
        let mut out = HashMap::new();
        for (mid, pts) in acc {
            let centre = pts.iter().copied().fold(Vec3::ZERO, |a, b| a + b) / pts.len() as f32;
            let radius = pts
                .iter()
                .map(|q| (*q - centre).length())
                .fold(0.0f32, f32::max)
                .max(0.05)
                + 0.1;
            out.insert(mid, BSphere { centre, radius });
        }
        out
    }

    /// Per-step orientation relaxation (Sec. 5.2.3, App. A.1). For every
    /// non-root module, gradient-descend its orientation *relative to its
    /// parent* to minimize f_distribution = ω1·f_collisions + ω2·f_tropism.
    /// Running this once per step lets the crown progressively self-organize as
    /// it grows (cf. the reorganizing trees of Fig. 11), driving the
    /// intersection-volume ratio down (Fig. 15a).
    fn optimize_orientations(&mut self) {
        let p = self.params.clone();
        let pl = self.place();
        let spheres = Self::spheres_from(&pl);

        // Compute all updates against the current placement, then apply (so the
        // pass is a single synchronous relaxation step).
        let mut updates: Vec<(ModuleId, Quat)> = Vec::new();
        for mid in self.pre_order(self.root) {
            let m = self.module(mid);
            let par = match m.parent {
                Some(par) => par,
                None => continue, // root orientation is fixed (grows up)
            };
            // Only developing modules reorient (App. A.1 optimizes *new*
            // modules). Once mature a module freezes, so settled structure
            // stops moving — younger neighbours do the dodging. This is what
            // keeps the crown from flickering every step.
            if p.opt_freeze_settled && m.age >= p.a_mature {
                continue;
            }
            let parent_world = pl.world_orient[&par];
            let base = pl.module_base[&mid];
            let proto = &self.protos[m.proto];
            let local_c = proto.local_centroid();
            let local_r = proto.local_radius();

            // Exclusions: self, parent, and children (structurally adjacent).
            let mut exclude: Vec<ModuleId> = vec![mid, par];
            exclude.extend(m.children.iter().map(|(_, c)| *c));

            // Skip modules that aren't actually colliding — there is nothing to
            // resolve, so leave them put. This freezes spread-out tips and is
            // what finally removes the flicker.
            if p.opt_freeze_settled {
                if let Some(&su) = spheres.get(&mid) {
                    let mut cur = 0.0;
                    for (&o, &so) in &spheres {
                        if exclude.contains(&o) {
                            continue;
                        }
                        cur += sphere_intersection_volume(su, so);
                    }
                    if cur < p.opt_collision_eps {
                        continue;
                    }
                }
            }

            let cost = |rel: Quat| -> f32 {
                let w = parent_world * rel;
                let sphere = BSphere {
                    centre: base + w * local_c,
                    radius: local_r,
                };
                let mut f_col = 0.0;
                for (&o, &so) in &spheres {
                    if exclude.contains(&o) {
                        continue;
                    }
                    f_col += sphere_intersection_volume(sphere, so);
                }
                let axis = (w * Vec3::Y).normalize_or_zero();
                let f_trop = (p.tropism_set_angle.cos() - axis.dot(Vec3::Y)).abs();
                p.omega1 * f_col + p.omega2 * f_trop
            };

            let mut rel = m.orientation;
            let mut best = cost(rel);
            for _ in 0..p.opt_iters {
                let candidates = [
                    rel * Quat::from_rotation_x(p.opt_angle),
                    rel * Quat::from_rotation_x(-p.opt_angle),
                    rel * Quat::from_rotation_z(p.opt_angle),
                    rel * Quat::from_rotation_z(-p.opt_angle),
                ];
                let mut improved = false;
                for c in candidates {
                    let cc = cost(c);
                    if cc < best - 1e-6 {
                        best = cc;
                        rel = c;
                        improved = true;
                    }
                }
                if !improved {
                    break;
                }
            }

            // Under-relaxation: move only a fraction of the way toward the
            // optimized orientation, capped at opt_max_step radians. This damps
            // the simultaneous-update oscillation and lets the crown settle.
            let current = m.orientation;
            let target = rel.normalize();
            let angle = current.angle_between(target);
            let damped = if angle > 1e-5 {
                let s = p.opt_damping.min(p.opt_max_step / angle).clamp(0.0, 1.0);
                current.slerp(target, s)
            } else {
                current
            };
            updates.push((mid, damped));
        }

        for (mid, rel) in updates {
            self.module_mut(mid).orientation = rel;
        }
    }
}

/// A bounding sphere for a module.
#[derive(Clone, Copy, Debug)]
pub struct BSphere {
    pub centre: Vec3,
    pub radius: f32,
}

/// Internal placement of the whole plant as a flat global node graph.
struct Placement {
    pos: Vec<Vec3>,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    /// Module id that each global node belongs to.
    gid_module: Vec<ModuleId>,
    diam: Vec<f32>,
    /// World-space orientation of each module (relative orientations composed).
    world_orient: HashMap<ModuleId, Quat>,
    /// World-space position of each module's root node.
    module_base: HashMap<ModuleId, Vec3>,
}

/// Voronoi-nearest prototype to a morphospace query point (λ, D).
fn nearest_proto(protos: &[Prototype], lambda: f32, d: f32) -> usize {
    let mut best = 0;
    let mut best_d2 = f32::INFINITY;
    for (i, proto) in protos.iter().enumerate() {
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

/// Volume of the intersection (lens) of two spheres.
pub fn sphere_intersection_volume(a: BSphere, b: BSphere) -> f32 {
    let d = (a.centre - b.centre).length();
    let (r1, r2) = (a.radius, b.radius);
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        // One sphere contained in the other.
        let r = r1.min(r2);
        return 4.0 / 3.0 * std::f32::consts::PI * r * r * r;
    }
    // Standard sphere-sphere lens volume.
    let sum = r1 + r2;
    std::f32::consts::PI * (sum - d) * (sum - d)
        * (d * d + 2.0 * d * sum - 3.0 * (r1 - r2) * (r1 - r2))
        / (12.0 * d)
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

    fn grow_with(params: PlantParams, steps: u32) -> Plant {
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
    fn orientation_optimization_reduces_collisions() {
        // Sec. 5.2.3 / Fig. 15a: the environment-sensitive model must reduce the
        // module intersection-volume ratio relative to the naive one. Measured
        // in a deliberately dense, bushy crown (short segments, many modules) so
        // there are real collisions to resolve.
        let dense = |optimize: bool| {
            let mut p = PlantParams::default();
            p.lambda = 0.5;
            p.l_max = 0.6;
            p.v_root_max = 120.0;
            p.v_max = 28.0;
            p.collision_light = optimize;
            p.optimize_orientation = optimize;
            grow_with(p, 120).intersection_ratio()
        };
        let naive = dense(false);
        let optimized = dense(true);

        assert!(
            optimized < 0.75 * naive,
            "optimization should clearly cut overlap: naive {naive:.3} -> optimized {optimized:.3}"
        );
        assert!(
            optimized < 0.05,
            "optimized ratio {optimized:.3} should stay under the 5% target"
        );
    }

    #[test]
    fn settled_modules_do_not_flicker() {
        // With the freeze/damping fix, mature modules in a crowded (bushy) crown
        // must stop moving — no perpetual back-and-forth re-orientation.
        let mut p = PlantParams::default();
        p.lambda = 0.30;
        p.collision_light = true;
        p.optimize_orientation = true;
        let mut plant = grow_with(p, 60);

        let mut last = plant.mature_centroids();
        let mut path = 0.0f32;
        for _ in 0..20 {
            plant.step(1.0);
            let now = plant.mature_centroids();
            for (id, p0) in &last {
                if let Some(p1) = now.get(id) {
                    path += (*p1 - *p0).length();
                }
            }
            last = now;
        }
        let per = path / last.len().max(1) as f32;
        assert!(per < 0.05, "settled modules still flicker: {per:.4} units/module");
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
