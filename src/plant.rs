//! Plant architecture and growth — the self-organizing **metamer model** of
//! Pałubicki et al. 2009, the foundation Makowski 2019 ("Synthetic
//! Silviculture") builds its plant scale on.
//!
//! A tree is a set of *metamers* (an internode + an axillary lateral bud; the
//! tip of an axis also carries a terminal bud). Each simulation cycle
//! (Pałubicki Fig. 3) is:
//!
//!   1. environment — space colonization (§4.1): buds compete for free-space
//!      marker points; a bud has Q>0 only where unoccupied markers remain in
//!      its perception cone, and grows toward them (V). This bounds each axis
//!      and fills the crown — the leader stops once the space above is claimed.
//!      Q is scaled by the global-shadow light g for inter-plant competition;
//!   2. light pass  — Q flows basipetally, accumulating Q_acc in internodes;
//!   3. vigor pass  — resource v = α·Q_base flows acropetally, split at each
//!      branch between the main axis and the lateral by the extended
//!      Borchert–Honda rule
//!          vm = v·λ·Qm / (λ·Qm + (1−λ)·Ql),   vl = v·(1−λ)·Ql / (…);
//!   4. bud fate    — a bud receiving resource v sprouts a shoot of n = ⌊v⌋
//!      metamers, each of length l = v/n, so **shoot length ∝ vigor** (this is
//!      what fills out crowns and closes a forest canopy);
//!   5. shedding    — a branch whose light-per-internode ratio is too low is
//!      dropped (forms clean boles under shade);
//!   6. diameters   — pipe model d = √(Σ d_child²), φ at the tips.
//!
//! Apical control λ around 0.5 spans the excurrent↔decurrent range (Pałubicki
//! Fig. 7): λ>0.5 favours the leader (excurrent), λ<0.5 the laterals
//! (decurrent). There is no fixed module prototype or morphospace — branching
//! is procedural and emerges from the competition for light.

use glam::Vec3;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

/// Stable id of an internode (kept named `ModuleId` for the ecosystem API).
pub type ModuleId = usize;

/// Golden-angle phyllotaxis (≈137.5°): successive lateral buds spiral around
/// the axis, so a shoot's branches fan out in 3D rather than stacking.
const GOLDEN_ANGLE: f32 = 2.399_963_2;

/// Maximum metamers a single bud sprouts in one step. The metamer rule is
/// n = ⌊v⌋ (Pałubicki §4.2); we cap it per step so a bud that funnels a large
/// resource extends gradually (and competition can redirect it between steps)
/// rather than extruding one long straight beam in a single cycle.
pub(crate) const MAX_SHOOT: u32 = 2;

/// A render-ready truncated-cone branch segment in world space.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub a: Vec3,
    pub b: Vec3,
    pub ra: f32,
    pub rb: f32,
}

/// Per plant-type parameters of the metamer model.
#[derive(Clone, Debug)]
pub struct PlantParams {
    /// Apical control λ (extended Borchert–Honda). λ>0.5 biases resource to the
    /// main axis (excurrent), λ<0.5 to the lateral (decurrent). The expressive
    /// range sits near 0.5 (Pałubicki Fig. 7).
    pub lambda: f32,
    /// Resource coefficient α: total resource v_base = α·Q_base (Pałubicki §4.2,
    /// typically ≈2). More α → longer shoots (n = ⌊v⌋) → denser, faster trees.
    pub alpha: f32,
    /// Plant-scale growth multiplier on the resource (species vigor knob).
    pub gp: f32,
    /// Cap on the total resource v_base. Climate adaptation scales this, so a
    /// poorly-adapted plant has a smaller resource budget. Also bounds tree size.
    pub v_root_max: f32,
    /// World length of a unit (l=1) internode; an internode's length is
    /// internode_len · (v/n).
    pub internode_len: f32,
    /// Lateral branching angle bias: high determinacy → narrower (more upright,
    /// excurrent) laterals; low → wider (more horizontal, decurrent).
    pub determinacy: f32,
    /// Pipe-model tip diameter contributed by each leaf (φ, Eq. 8).
    pub phi: f32,
    /// Tropism temporal decay (g1): younger metamers in a shoot bend more.
    pub g1: f32,
    /// Lateral gravitropism (g2): negative droops branches down, positive lifts
    /// them up. Shapes the spread of the crown (Pałubicki Fig. 12).
    pub g2: f32,
    /// Leader up-righting strength: how strongly terminal (axis-continuing)
    /// shoots steer back toward vertical, keeping a straight bole.
    pub tropism_up: f32,
    /// Optimal-direction weight ξ (Pałubicki §4.2): a continuing shoot's heading
    /// is a weighted sum of the *default orientation* (the parent axis heading,
    /// implicit weight 1) and the optimal growth direction V (weight ξ). ξ<1
    /// keeps axes stiff — they bend gently toward free space rather than snapping
    /// to V each step. Without this term (pure V) axes wander/wiggle like worms,
    /// because V jitters as markers are consumed and neighbours compete.
    pub xi: f32,
    /// Senescence onset age (p_max): past it the resource ramps to zero.
    pub p_max: f32,
    /// Shade tolerance s_tol ∈ [0,1]: the global-shadow light floor,
    /// Q = lerp(s_tol, 1, Q_G).
    pub shade_tolerance: f32,
    /// Shedding threshold: a lateral branch whose light-per-internode ratio
    /// falls below this is shed (Pałubicki §4.4). Low = gentle pruning.
    pub shed_ratio: f32,

    // --- space colonization (Pałubicki §4.1): the environment that bounds and
    //     shapes growth. Buds compete for free-space marker points within a
    //     dome-shaped cloud; a bud only grows where unoccupied markers remain
    //     in its perception cone, so the leader stops once the space above is
    //     claimed and branches grow into the open. ---
    /// Marker-cloud envelope: dome height (max tree height) and base radius
    /// (max crown spread). The tree fills this and stops.
    pub envelope_height: f32,
    pub envelope_radius: f32,
    /// Number of free-space markers seeded in the dome.
    pub marker_count: usize,
    /// Occupancy radius ρ: markers within ρ of any bud are consumed.
    pub occupancy_radius: f32,
    /// Perception radius r: how far a bud senses free markers.
    pub perception_radius: f32,
    /// Perception cone: a marker counts only if cos(angle to the bud's facing)
    /// ≥ this (≈ a 70–90° forward cone).
    pub perception_cos: f32,
    /// Vertical growth rate: the reachable-marker "ceiling" rises this many
    /// world-units per step, so the tree fills its envelope gradually (bottom-up,
    /// like a crown rising with age) instead of consuming the whole cloud at
    /// once. Lower = slower, more watchable growth; the final form is unchanged.
    pub climb_rate: f32,
    /// Hard cap on internode count (bounds geometry / performance).
    pub max_modules: usize,
}

impl Default for PlantParams {
    fn default() -> Self {
        Self {
            lambda: 0.52,
            alpha: 2.0,
            gp: 1.0,
            v_root_max: 120.0,
            internode_len: 0.55,
            determinacy: 0.5,
            phi: 0.04,
            g1: 1.6,
            g2: -0.18,
            tropism_up: 0.30,
            xi: 0.25,
            p_max: 1.0e9,
            shade_tolerance: 0.0,
            shed_ratio: 0.35, // shed lateral branches whose mean light < 0.35
            envelope_height: 14.0,
            envelope_radius: 4.0,
            marker_count: 800,
            occupancy_radius: 1.1,
            perception_radius: 2.8,
            perception_cos: 0.3, // ≈ 72° half-angle forward cone
            climb_rate: 0.3,
            max_modules: 4000,
        }
    }
}

/// One metamer: the internode from `base` along `dir` for `length`, plus the
/// buds it carries. An axis is a chain of metamers of equal `order`.
#[derive(Clone, Debug)]
struct Internode {
    parent: Option<ModuleId>,
    children: Vec<ModuleId>,
    base: Vec3,
    dir: Vec3,
    length: f32,
    /// Axis order: 0 is the trunk; a lateral starts an axis of order+1.
    order: u32,
    /// Index of this metamer within its shoot (drives tropism decay & phyllo).
    rank: u32,
    age: f32,
    /// This metamer's tip carries the terminal bud (continues the axis).
    terminal_bud: bool,
    /// This metamer carries an unspouted axillary (lateral) bud.
    lateral_bud: bool,
    // --- recomputed each cycle ---
    /// Space-and-light availability Q at this metamer's buds (0 if no free
    /// space in the perception cone, else the local light level).
    q_bud: f32,
    /// Optimal growth direction toward free space (Pałubicki §4.1 V vector);
    /// zero when no free markers are perceived.
    v_grow: Vec3,
    /// Basipetally accumulated light through this internode's subtree.
    q_acc: f32,
    /// Current light exposure (the global-shadow level g, independent of free
    /// space). Shedding (Pałubicki §4.4) drops a lateral branch whose mean
    /// light_level is low — i.e. one that has been overtopped/shaded — which
    /// clears lower branches into a clean bole. Age-independent (unlike a
    /// cumulative sum), so single open-grown trees (g=1) never self-prune.
    light_level: f32,
    /// Resource v reaching this internode (acropetal).
    vigor: f32,
    /// Resource routed to the terminal / lateral bud this cycle.
    term_resource: f32,
    lat_resource: f32,
    /// Pipe-model diameter.
    diam: f32,
}

impl Internode {
    fn tip(&self) -> Vec3 {
        self.base + self.dir * self.length
    }
}

pub struct Plant {
    /// Internode storage; `None` slots are shed metamers (ids stay stable).
    nodes: Vec<Option<Internode>>,
    /// Count of live (`Some`) slots — `module_count()` in O(1).
    live: usize,
    /// Freed (shed) slots, min-first, so `alloc` reuses the lowest free index
    /// (matching the old linear `position(is_none)` scan) in O(log n) instead
    /// of O(n) — the latter made a whole growth O(n²).
    free: BinaryHeap<Reverse<ModuleId>>,
    root: ModuleId,
    pub params: PlantParams,
    /// Plant age p_t (simulation steps / "years").
    pub age: f32,
    pub origin: Vec3,
    /// Live free-space markers (Pałubicki §4.1); consumed as the tree grows.
    markers: Vec<Vec3>,
    /// Smoothed carbon balance: an EMA of mean *raw* light captured per metamer.
    /// When it stays low the plant can't pay its upkeep and is culled
    /// (carbon-starvation mortality). Raw (not floored by shade tolerance) so the
    /// carbon signal reflects light actually captured — tolerance instead lowers
    /// the survival *bar* and costs growth (see `cull_dead` / the vigor penalty).
    /// Starts healthy; the EMA prevents a transient shading from killing a tree.
    health: f32,
    /// Mean raw light `g` over live metamers this step (feeds `health`).
    mean_g: f32,
}

impl Plant {
    pub fn new(params: PlantParams, origin: Vec3) -> Self {
        // Seedling: one upright internode of the trunk axis, carrying both a
        // terminal bud (to extend the trunk) and a lateral bud.
        let seed = Internode {
            parent: None,
            children: Vec::new(),
            base: origin,
            dir: Vec3::Y,
            length: params.internode_len,
            order: 0,
            rank: 0,
            age: 0.0,
            terminal_bud: true,
            lateral_bud: true,
            q_bud: 1.0,
            v_grow: Vec3::Y,
            q_acc: 1.0,
            light_level: 0.0,
            vigor: 0.0,
            term_resource: 0.0,
            lat_resource: 0.0,
            diam: params.phi,
        };
        let markers = generate_markers(
            origin,
            params.envelope_radius,
            params.envelope_height,
            params.marker_count,
        );
        Plant {
            nodes: vec![Some(seed)],
            live: 1,
            free: BinaryHeap::new(),
            root: 0,
            params,
            age: 0.0,
            origin,
            markers,
            health: 1.0,
            mean_g: 1.0,
        }
    }

    /// Smoothed carbon balance (resource captured per metamer, EMA). Low ⇒ the
    /// plant cannot pay its upkeep (carbon starvation).
    pub fn health(&self) -> f32 {
        self.health
    }

    /// Resource (vigor) reaching the root this cycle — the plant's overall growth
    /// budget. Compared to `params.v_root_max` it gives how vigorous the plant is
    /// (used for vigor-scaled maturity: vigorous plants flower sooner).
    pub fn root_vigor(&self) -> f32 {
        self.node(self.root).vigor
    }

    fn node(&self, id: ModuleId) -> &Internode {
        self.nodes[id].as_ref().unwrap()
    }
    fn node_mut(&mut self, id: ModuleId) -> &mut Internode {
        self.nodes[id].as_mut().unwrap()
    }
    fn alive_ids(&self) -> Vec<ModuleId> {
        (0..self.nodes.len()).filter(|&i| self.nodes[i].is_some()).collect()
    }
    pub fn module_count(&self) -> usize {
        self.live
    }

    /// Advance the simulation one step (standalone plant: own marker dome,
    /// self-shading).
    pub fn step(&mut self, dt: f32) {
        self.step_impl(dt, None, None);
    }

    /// Advance using the ecosystem's SHARED marker field — `space` is the
    /// per-module growth direction V each bud captured (presence ⇒ free space),
    /// `qg` the per-module global-shadow light g.
    pub fn step_in_field(
        &mut self,
        dt: f32,
        qg: &FxIdMap<f32>,
        space: &FxIdMap<Vec3>,
    ) {
        self.step_impl(dt, Some(qg), Some(space));
    }

    fn step_impl(
        &mut self,
        dt: f32,
        qg: Option<&FxIdMap<f32>>,
        space: Option<&FxIdMap<Vec3>>,
    ) {
        self.age += dt;
        for id in self.alive_ids() {
            self.node_mut(id).age += dt;
        }
        self.environment(qg, space);
        self.light_pass();
        self.vigor_pass();
        // Carbon balance: the mean RAW light the foliage receives, smoothed
        // (size-independent). Tolerance does not inflate this — it instead lowers
        // the survival bar (in `cull_dead`) and costs growth (the vigor penalty),
        // so the engine of succession is a genuine tradeoff, not free health.
        self.health = self.health * 0.9 + self.mean_g * 0.1;
        self.grow();
        self.shed();
        self.recompute_diameters();
    }

    // --- 1. environment: space colonization (Pałubicki §4.1) -----------------
    // `space`, when supplied (ecosystem), gives the per-module growth direction V
    // from the SHARED marker field — its presence means the bud captured free
    // space (Q>0). When None (standalone plant) the plant colonizes its own
    // private marker dome. `qg`, when supplied, is the global-shadow light g; else
    // the plant self-shades. Each bud's Q = space-presence × light g; a bud with
    // no free space gets Q=0 and stops (bounding the leader, filling the crown).
    fn environment(
        &mut self,
        qg: Option<&FxIdMap<f32>>,
        space: Option<&FxIdMap<Vec3>>,
    ) {
        let stol = self.params.shade_tolerance;
        let ids = self.alive_ids();

        // Growth-direction V per module: supplied (shared field) or self-colonized.
        let local_space;
        let space_map: &FxIdMap<Vec3> = match space {
            Some(s) => s,
            None => {
                local_space = self.colonize_self();
                &local_space
            }
        };
        // Light g: supplied global shadow, else self-shadow (standalone plant).
        let self_g = if qg.is_none() { Some(self.self_shadow()) } else { None };

        let mut tip_g = 0.0f32; // raw light over the crown tips (the foliage)
        let mut tip_n = 0u32;
        for &id in &ids {
            let g = qg
                .and_then(|map| map.get(&id).copied())
                .or_else(|| self_g.as_ref().and_then(|map| map.get(&id).copied()))
                .unwrap_or(1.0);
            // Carbon comes from the foliage — the crown TIPS (childless metamers).
            // Their mean light naturally DECLINES as a crown grows larger/denser
            // (self-shading), so a big crown only pays its way where productivity
            // is high: this size↔light diminishing return is what lets climate
            // select morphology. Tolerance does not inflate it (raw g) — it only
            // lowers the survival bar and costs growth.
            if self.node(id).children.is_empty() {
                tip_g += g;
                tip_n += 1;
            }
            // light_level (floored by tolerance) drives GROWTH and shedding — a
            // tolerant plant keeps extending / holding branches in shade — but the
            // carbon balance below uses raw g, so tolerance is not free survival.
            let light = stol + (1.0 - stol) * g;
            self.node_mut(id).light_level = light;
            match space_map.get(&id) {
                Some(v) => {
                    self.node_mut(id).q_bud = light;
                    self.node_mut(id).v_grow = *v;
                }
                None => {
                    self.node_mut(id).q_bud = 0.0;
                    self.node_mut(id).v_grow = Vec3::ZERO;
                }
            }
        }
        self.mean_g = if tip_n > 0 { tip_g / tip_n as f32 } else { 0.0 };
    }

    /// Active-bud tips (module id, position, facing direction) — what the
    /// ecosystem gathers to colonize the shared marker field.
    pub fn active_buds(&self) -> Vec<(ModuleId, Vec3, Vec3)> {
        self.alive_ids()
            .into_iter()
            .filter_map(|id| {
                let n = self.node(id);
                (n.terminal_bud || n.lateral_bud).then(|| (id, n.tip(), n.dir.normalize_or_zero()))
            })
            .collect()
    }

    /// Reachable-marker ceiling: rises with age (gradual bottom-up growth) but
    /// caps at the species height (envelope_height), so a tree stops growing up
    /// at its genetic maximum even in a tall shared marker field — this keeps
    /// species height differentiation in the ecosystem.
    pub fn reveal_ceiling(&self) -> f32 {
        let rise = 2.0 * self.params.internode_len + self.age * self.params.climb_rate;
        self.origin.y + rise.min(self.params.envelope_height + self.params.internode_len)
    }

    pub fn clear_markers(&mut self) {
        self.markers.clear();
    }

    /// Colonize the plant's OWN private marker dome (standalone mode), consuming
    /// reached markers; returns the per-module growth direction V.
    fn colonize_self(&mut self) -> FxIdMap<Vec3> {
        let p = self.params.clone();
        let ceiling = self.reveal_ceiling();
        let crown_r = p.envelope_radius + p.internode_len;
        let origin = self.origin;
        let active = self.active_buds();
        let buds: Vec<BudQuery> = active
            .iter()
            .map(|&(_, pos, dir)| BudQuery { pos, dir, ceiling, center: origin, crown_r })
            .collect();
        let vs = colonize(
            &mut self.markers,
            Occ::Consume,
            &buds,
            p.occupancy_radius,
            p.perception_radius,
            p.perception_cos,
            None,
        );
        let mut out = FxIdMap::default();
        for (i, v) in vs.into_iter().enumerate() {
            if let Some(dir) = v {
                out.insert(active[i].0, dir);
            }
        }
        out
    }

    /// Per-internode self-shadow light for a standalone plant: a downward
    /// pyramidal-penumbra voxel grid over the plant's own metamer tips
    /// (Pałubicki §4.1 shadow propagation; g = max(C − s + a, 0)/C, the +a
    /// cancels a tip's self-shadow). Lower/interior tips sit under more shadow,
    /// so they get less light → shed and self-thin like a forest tree.
    fn self_shadow(&self) -> FxIdMap<f32> {
        let cell = (self.params.internode_len * 2.0).max(0.5);
        let inv = 1.0 / cell;
        // Gentle: a shallow penumbra (small qmax) and a high full-exposure
        // constant C, so deeply-buried interior tips are thinned but exposed
        // ones (e.g. all of a narrow column) keep near-full light.
        let (a, b, c, qmax) = (1.0f32, 2.0f32, 16.0f32, 3i32);
        let key = |p: Vec3| {
            pack(((p.x * inv).floor() as i32, (p.y * inv).floor() as i32, (p.z * inv).floor() as i32))
        };
        let ids = self.alive_ids();
        let mut s: FxMap<f32> = FxMap::default();
        for &id in &ids {
            let (ci, cj, ck) = grid_key(self.node(id).tip(), inv);
            for q in 0..=qmax {
                let j = cj - q; // shadow propagates downward
                let ds = a * b.powi(-q);
                for di in -q..=q {
                    for dk in -q..=q {
                        *s.entry(pack((ci + di, j, ck + dk))).or_insert(0.0) += ds;
                    }
                }
            }
        }
        let mut out = FxIdMap::default();
        for &id in &ids {
            let sv = s.get(&key(self.node(id).tip())).copied().unwrap_or(0.0);
            out.insert(id, ((c - sv + a).max(0.0) / c).clamp(0.0, 1.0));
        }
        out
    }

    /// Light gathered locally at a metamer: its active buds' availability Q
    /// (which is 0 once the surrounding space is consumed, so resource tracks
    /// the growing surface, not the interior).
    fn q_self(&self, id: ModuleId) -> f32 {
        let n = self.node(id);
        ((n.terminal_bud as u32 + n.lateral_bud as u32) as f32) * n.q_bud
    }

    // --- 2. basipetal light accumulation -------------------------------------
    //   Q_acc(u) = Q_self(u) + Σ Q_acc(child), accumulated tip-to-base.
    fn light_pass(&mut self) {
        for &id in &self.post_order(self.root) {
            let mut acc = self.q_self(id);
            let nc = self.node(id).children.len();
            for ci in 0..nc {
                let c = self.node(id).children[ci];
                acc += self.node(c).q_acc;
            }
            self.node_mut(id).q_acc = acc;
        }
    }

    // --- 3. acropetal Borchert–Honda resource distribution -------------------
    fn vigor_pass(&mut self) {
        let p = self.params.clone();
        let senescence = if self.age <= p.p_max {
            1.0
        } else {
            (1.0 - (self.age - p.p_max) / p.p_max.max(1.0)).clamp(0.0, 1.0)
        };
        let q_base = self.node(self.root).q_acc;
        // Tolerance↔growth tradeoff: a shade-tolerant plant (cheap, durable
        // leaves that subsist on low light) grows more slowly. So tolerance buys
        // survival in shade (a lower carbon bar) at the cost of losing the
        // height/light race — it is not a free advantage.
        let tol_cost = 1.0 - 0.5 * p.shade_tolerance;
        let v_base = (p.gp * p.alpha * q_base * tol_cost).min(p.v_root_max) * senescence;

        // Zero all routing state first, then seed the root. A node whose whole
        // subtree is dark (denom = 0) routes nothing, so its children must read
        // 0 here rather than a stale value from a previous step (else resource
        // leaks and conservation breaks).
        for id in self.alive_ids() {
            let n = self.node_mut(id);
            n.vigor = 0.0;
            n.term_resource = 0.0;
            n.lat_resource = 0.0;
        }
        self.node_mut(self.root).vigor = v_base;

        for &id in &self.pre_order(self.root) {
            let v = self.node(id).vigor;
            let order = self.node(id).order;

            // Each internode has at most one "main" outgoing branch (the axis
            // continuation: a same-order child, else the terminal bud) and one
            // "lateral" (a higher-order child, else the lateral bud).
            let mut main_child = None;
            let mut lat_child = None;
            for &c in &self.node(id).children {
                if self.node(c).order == order {
                    main_child = Some(c);
                } else {
                    lat_child = Some(c);
                }
            }
            let q_bud = self.node(id).q_bud;
            let has_term = self.node(id).terminal_bud;
            let has_lat = self.node(id).lateral_bud;

            let main_q = main_child
                .map(|c| self.node(c).q_acc)
                .or(if has_term { Some(q_bud) } else { None });
            let lat_q = lat_child
                .map(|c| self.node(c).q_acc)
                .or(if has_lat { Some(q_bud) } else { None });

            let wm = main_q.map(|q| p.lambda * q).unwrap_or(0.0);
            let wl = lat_q.map(|q| (1.0 - p.lambda) * q).unwrap_or(0.0);
            let denom = wm + wl;
            if denom <= 1e-9 {
                continue;
            }
            let main_share = v * wm / denom;
            let lat_share = v * wl / denom;

            match main_child {
                Some(c) => self.node_mut(c).vigor = main_share,
                None if has_term => self.node_mut(id).term_resource = main_share,
                _ => {}
            }
            match lat_child {
                Some(c) => self.node_mut(c).vigor = lat_share,
                None if has_lat => self.node_mut(id).lat_resource = lat_share,
                _ => {}
            }
        }
    }

    // --- 4. bud fate: sprout shoots of ⌊v⌋ metamers --------------------------
    fn grow(&mut self) {
        let p = self.params.clone();
        for id in self.alive_ids() {
            if self.module_count() >= p.max_modules {
                break;
            }
            // Terminal bud → continue the axis (same order). Pałubicki §4.2: the
            // new shoot's heading is a weighted sum of the DEFAULT ORIENTATION
            // (the parent axis heading, weight 1) and the optimal growth
            // direction V toward free space (weight ξ). The default-orientation
            // term is the axis stiffness that keeps a bole straight; steering is
            // a gentle bend toward V, not a snap to it (which wiggles).
            let n = self.node(id);
            if n.terminal_bud {
                let v = n.term_resource;
                if v >= 1.0 {
                    let order = n.order;
                    let axis = n.dir.normalize_or_zero();
                    let dir = if n.v_grow.length_squared() > 1e-6 {
                        (axis + n.v_grow.normalize_or_zero() * p.xi).normalize_or_zero()
                    } else {
                        axis
                    };
                    self.node_mut(id).terminal_bud = false;
                    self.sprout(id, dir, order, v, true);
                }
            }
            if self.module_count() >= p.max_modules {
                break;
            }
            // Lateral bud → start a new axis (order+1) at a branching angle.
            let n = self.node(id);
            if n.lateral_bud {
                let v = n.lat_resource;
                if v >= 1.0 {
                    let order = n.order + 1;
                    let dir = self.lateral_direction(id);
                    self.node_mut(id).lateral_bud = false;
                    self.sprout(id, dir, order, v, false);
                }
            }
        }
    }

    /// Append a shoot of n = ⌊v⌋ metamers (each of length internode_len·v/n)
    /// from the tip of `parent`, in `start_dir`, bending by tropism each step.
    fn sprout(&mut self, parent: ModuleId, start_dir: Vec3, order: u32, v: f32, leader: bool) {
        let p = self.params.clone();
        // n = ⌊v⌋ metamers of length l = v/⌊v⌋ (Pałubicki §4.2), capped per step
        // at MAX_SHOOT (see the const).
        let want = v.floor().max(1.0);
        let n = (want as u32).min(MAX_SHOOT);
        let l = (v / want).clamp(1.0, 1.7); // unit-ish internode length
        let mut base = self.node(parent).tip();
        let mut dir = start_dir.normalize_or_zero();
        if dir.length_squared() < 1e-9 {
            dir = Vec3::Y;
        }
        let mut prev = parent;
        let parent_phyllo = (self.node(parent).rank as f32) * GOLDEN_ANGLE;
        for k in 0..n {
            if self.module_count() >= p.max_modules {
                break;
            }
            // Tropism: leaders right toward vertical; laterals droop/lift by g2.
            let decay = p.g1 / (k as f32 + p.g1);
            let pull = if leader { p.tropism_up } else { p.g2 };
            dir = (dir + Vec3::Y * (pull * decay)).normalize_or_zero();
            if dir.length_squared() < 1e-9 {
                dir = Vec3::Y;
            }
            let node = Internode {
                parent: Some(prev),
                children: Vec::new(),
                base,
                dir,
                length: p.internode_len * l,
                order,
                rank: k,
                age: 0.0,
                terminal_bud: k == n - 1,
                lateral_bud: true,
                q_bud: 1.0,
                v_grow: Vec3::ZERO,
                q_acc: 1.0,
                light_level: 0.0,
                vigor: 0.0,
                term_resource: 0.0,
                lat_resource: 0.0,
                diam: p.phi,
            };
            base = node.tip();
            let _ = parent_phyllo;
            let new_id = self.alloc(node);
            self.node_mut(prev).children.push(new_id);
            prev = new_id;
        }
    }

    /// Direction of a metamer's lateral bud: the parent axis tilted by the
    /// branching angle (from determinacy) around the phyllotactic azimuth.
    fn lateral_direction(&self, id: ModuleId) -> Vec3 {
        let n = self.node(id);
        let d = n.dir.normalize_or_zero();
        // High determinacy → narrow angle (upright, excurrent); low → wide.
        let angle = (30.0 + (1.0 - self.params.determinacy) * 50.0).to_radians();
        let azimuth = (n.rank as f32) * GOLDEN_ANGLE;
        let (u, v) = d.any_orthonormal_pair();
        let radial = (u * azimuth.cos() + v * azimuth.sin()).normalize_or_zero();
        (d * angle.cos() + radial * angle.sin()).normalize_or_zero()
    }

    // --- 5. shedding: drop starved lateral branches (Pałubicki §4.4) ---------
    // A lateral branch whose mean current light falls below the threshold is a
    // net liability and is shed — overtopped/shaded lower branches drop, leaving
    // a clean bole. Uses the light *level* (g), not q_acc (which is ~0 once a
    // mature tree has spent its markers) and not a cumulative sum (which would
    // conflate a young lit branch with an old shaded one).
    fn shed(&mut self) {
        let p = self.params.clone();
        if p.shed_ratio <= 0.0 {
            return;
        }
        // Subtree internode count and summed current light (post-order); the
        // ratio light/size is the branch's mean light level. Indexed by id (a
        // dense slot index) — post-order visits children before parents, so a
        // child's entry is always set before its parent reads it.
        let order = self.post_order(self.root);
        let mut size: Vec<u32> = vec![0; self.nodes.len()];
        let mut light: Vec<f32> = vec![0.0; self.nodes.len()];
        for &id in &order {
            let mut s = 1u32;
            let mut lt = self.node(id).light_level;
            let nc = self.node(id).children.len();
            for ci in 0..nc {
                let c = self.node(id).children[ci];
                s += size[c];
                lt += light[c];
            }
            size[id] = s;
            light[id] = lt;
        }
        // Candidates: the base of a lateral axis (parent of lower order), old
        // enough to have had a fair chance to gather. Never the trunk (order 0).
        let mut to_shed: Vec<ModuleId> = Vec::new();
        for &id in &order {
            let n = self.node(id);
            if n.order == 0 || n.age < 12.0 {
                continue;
            }
            let is_axis_base = n.parent.map(|pp| self.node(pp).order < n.order).unwrap_or(false);
            if !is_axis_base {
                continue;
            }
            if light[id] / (size[id] as f32) < p.shed_ratio {
                to_shed.push(id);
            }
        }
        for id in to_shed {
            if self.nodes[id].is_some() {
                self.remove_subtree(id);
            }
        }
    }

    fn remove_subtree(&mut self, id: ModuleId) {
        if let Some(parent) = self.node(id).parent {
            if let Some(pm) = self.nodes[parent].as_mut() {
                pm.children.retain(|c| *c != id);
            }
        }
        let mut stack = vec![id];
        let mut dead = Vec::new();
        while let Some(cur) = stack.pop() {
            if let Some(m) = self.nodes[cur].as_ref() {
                for &c in &m.children {
                    stack.push(c);
                }
                dead.push(cur);
            }
        }
        for d in dead {
            self.nodes[d] = None;
            self.free.push(Reverse(d));
            self.live -= 1;
        }
    }

    // --- 6. pipe-model diameters ---------------------------------------------
    fn recompute_diameters(&mut self) {
        let phi = self.params.phi;
        for &id in &self.post_order(self.root) {
            let nc = self.node(id).children.len();
            let d = if nc == 0 {
                phi
            } else {
                let mut sum = 0.0f32;
                for ci in 0..nc {
                    let c = self.node(id).children[ci];
                    sum += self.node(c).diam.powi(2);
                }
                sum.sqrt().max(phi)
            };
            self.node_mut(id).diam = d;
        }
    }

    // --- storage / traversal helpers -----------------------------------------
    fn alloc(&mut self, n: Internode) -> ModuleId {
        self.live += 1;
        if let Some(Reverse(slot)) = self.free.pop() {
            self.nodes[slot] = Some(n);
            slot
        } else {
            self.nodes.push(Some(n));
            self.nodes.len() - 1
        }
    }

    fn pre_order(&self, root: ModuleId) -> Vec<ModuleId> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            out.push(id);
            for &c in &self.node(id).children {
                stack.push(c);
            }
        }
        out
    }

    fn post_order(&self, root: ModuleId) -> Vec<ModuleId> {
        let mut pre = self.pre_order(root);
        pre.reverse();
        pre
    }

    // --- geometry / queries (public API consumed by mesh, ecosystem, main) ---

    /// Render skeleton: one truncated cone per internode, tapering from its own
    /// (pipe-model) diameter at the base toward its children's at the tip.
    pub fn skeleton(&self) -> Vec<Segment> {
        let mut segs = Vec::new();
        for id in self.alive_ids() {
            let n = self.node(id);
            if n.length < 1e-5 {
                continue;
            }
            let kids = &n.children;
            let tip_d = if kids.is_empty() {
                self.params.phi
            } else {
                kids.iter().map(|&c| self.node(c).diam).fold(0.0, f32::max)
            };
            segs.push(Segment {
                a: n.base,
                b: n.tip(),
                ra: n.diam * 0.5,
                rb: tip_d * 0.5,
            });
        }
        segs
    }

    /// Leaf attachment points (world position, outward direction): every thin
    /// twig (diameter near φ) that has extended. Foliage clusters render here.
    pub fn leaves(&self) -> Vec<(Vec3, Vec3)> {
        let twig = self.params.phi * 2.5;
        let mut out = Vec::new();
        for id in self.alive_ids() {
            let n = self.node(id);
            if n.diam <= twig && n.length > 0.1 {
                out.push((n.tip(), n.dir));
            }
        }
        out
    }

    /// Highest point reached above the base.
    pub fn height(&self) -> f32 {
        self.alive_ids()
            .iter()
            .map(|&id| self.node(id).tip().y.max(self.node(id).base.y))
            .fold(0.0, f32::max)
            - self.origin.y
    }

    /// Total wood volume (Σ truncated-cone segment volumes) — a biomass proxy
    /// for the self-thinning validation.
    pub fn biomass(&self) -> f32 {
        self.skeleton()
            .iter()
            .map(|s| {
                let l = (s.b - s.a).length();
                std::f32::consts::PI / 3.0 * (s.ra * s.ra + s.ra * s.rb + s.rb * s.rb) * l
            })
            .sum()
    }

    /// Basal trunk diameter (the thickest segment radius × 2) — for allometry.
    pub fn trunk_diameter(&self) -> f32 {
        2.0 * self.skeleton().iter().map(|s| s.ra).fold(0.0, f32::max)
    }

    /// `(height, crown_radius, apex_offset)` about the plant's base: crown
    /// radius is the max horizontal reach; apex_offset is the highest node's
    /// horizontal distance from the trunk axis (how much the leader leans/arcs).
    pub fn shape(&self) -> (f32, f32, f32) {
        let base = self.origin;
        let mut height = 0.0f32;
        let mut crown_radius = 0.0f32;
        let mut best_y = f32::MIN;
        let mut apex_offset = 0.0f32;
        for id in self.alive_ids() {
            for pnt in [self.node(id).base, self.node(id).tip()] {
                let dx = pnt.x - base.x;
                let dz = pnt.z - base.z;
                let horiz = (dx * dx + dz * dz).sqrt();
                height = height.max(pnt.y - base.y);
                crown_radius = crown_radius.max(horiz);
                if pnt.y > best_y {
                    best_y = pnt.y;
                    apex_offset = horiz;
                }
            }
        }
        (height, crown_radius, apex_offset)
    }

    /// World-space centre of each internode (for shadow-grid deposition and the
    /// reciprocal Q_G lookup). Keyed by internode id.
    pub fn module_centres(&self) -> Vec<(ModuleId, Vec3)> {
        self.alive_ids()
            .into_iter()
            .map(|id| {
                let n = self.node(id);
                (id, (n.base + n.tip()) * 0.5)
            })
            .collect()
    }

    /// Per-internode bounding spheres (for the intersection diagnostic).
    pub fn module_spheres(&self) -> HashMap<ModuleId, BSphere> {
        let mut out = HashMap::new();
        for id in self.alive_ids() {
            let n = self.node(id);
            out.insert(
                id,
                BSphere {
                    centre: (n.base + n.tip()) * 0.5,
                    radius: (n.length * 0.5).max(0.05) + 0.05,
                },
            );
        }
        out
    }

    /// Summed non-adjacent internode intersection volume / total sphere volume
    /// (a self-overlap diagnostic).
    pub fn intersection_ratio(&self) -> f32 {
        let spheres = self.module_spheres();
        let ids: Vec<ModuleId> = spheres.keys().copied().collect();
        let mut inter = 0.0;
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let (a, b) = (ids[i], ids[j]);
                let adj = self.node(a).parent == Some(b) || self.node(b).parent == Some(a);
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
}

/// A bounding sphere for an internode.
#[derive(Clone, Copy, Debug)]
pub struct BSphere {
    pub centre: Vec3,
    pub radius: f32,
}

/// Seed a dome-shaped cloud of free-space markers above `origin` (Pałubicki
/// §4.1 set S). The dome is an upright half-ellipsoid of base radius `radius`
/// and height `height`; the tree grows into it and stops when the markers run
/// out, so the envelope bounds the mature tree. Deterministic per origin (a
/// small LCG seeded from the position) so a stand is reproducible.
fn generate_markers(origin: Vec3, radius: f32, height: f32, count: usize) -> Vec<Vec3> {
    let mut s: u64 = (origin.x.to_bits() as u64)
        .rotate_left(21)
        ^ (origin.z.to_bits() as u64).rotate_left(43)
        ^ 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((s >> 33) as f32) / ((1u64 << 31) as f32) // ∈ [0, 1)
    };
    let mut out = Vec::with_capacity(count);
    let mut tries = 0usize;
    while out.len() < count && tries < count.saturating_mul(20).max(1000) {
        tries += 1;
        let x = next() * 2.0 - 1.0;
        let z = next() * 2.0 - 1.0;
        let y = next(); // 0..1 up the dome
        // Half-ellipsoid: keep points inside the dome of this height.
        if x * x + z * z <= 1.0 - y * y {
            out.push(origin + Vec3::new(x * radius, y * height, z * radius));
        }
    }
    out
}

/// FxHash-style hasher (rustc-hash): a single rotate-xor-multiply per key. The
/// spatial-hash and occupancy maps below are keyed by integer voxel ids; the
/// default SipHash is cryptographic and dominated `colonize`/`self_shadow`, so
/// we hash packed `u64` cell keys with this instead. ~zero extra cost, no deps.
#[derive(Default)]
pub(crate) struct FxHasher {
    h: u64,
}
const FX_K: u64 = 0x51_7c_c1_b7_27_22_0a_95;
impl std::hash::Hasher for FxHasher {
    /// splitmix64 finalizer: our keys are packed voxel triples, so a plain
    /// `key·K` would leave the bucket index (low bits) depending only on the
    /// lowest-packed coordinate — catastrophic clustering for a grid. The
    /// avalanche diffuses all bits into the low ones, so every coordinate
    /// affects the bucket. (Cheap: 2 mults, vs SipHash's full mixing.)
    fn finish(&self) -> u64 {
        let mut z = self.h;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn write_u64(&mut self, i: u64) {
        self.h = (self.h.rotate_left(5) ^ i).wrapping_mul(FX_K);
    }
    // Integer-key shortcuts so usize/u32 keys (module ids) don't fall through to
    // the byte-loop fallback below.
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }
    fn write_u32(&mut self, i: u32) {
        self.write_u64(i as u64);
    }
    fn write(&mut self, bytes: &[u8]) {
        // Generic fallback (unused on our integer keys, but keep it correct).
        for &b in bytes {
            self.write_u64(b as u64);
        }
    }
}
pub(crate) type BuildFx = std::hash::BuildHasherDefault<FxHasher>;
pub(crate) type FxMap<V> = HashMap<u64, V, BuildFx>;
/// Module-id-keyed map with the fast hasher (for per-plant `qg`/`space`/shed).
pub(crate) type FxIdMap<V> = HashMap<ModuleId, V, BuildFx>;

/// Pack a voxel coordinate triple into a single `u64` key (21 bits each, with a
/// +2^20 offset so the range is [-2^20, 2^20) per axis — far beyond any plot).
pub(crate) fn pack(c: (i32, i32, i32)) -> u64 {
    const B: i64 = 1 << 20;
    const M: u64 = (1 << 21) - 1;
    let (i, j, k) = c;
    ((i as i64 + B) as u64 & M)
        | (((j as i64 + B) as u64 & M) << 21)
        | (((k as i64 + B) as u64 & M) << 42)
}

/// A small spatial hash over points (cell-bucketed indices) for near-linear
/// neighbour queries during space colonization.
struct PointGrid {
    inv: f32,
    cells: FxMap<Vec<usize>>,
}
impl PointGrid {
    fn new(pts: &[Vec3], cell: f32) -> Self {
        let inv = 1.0 / cell.max(0.1);
        let mut cells: FxMap<Vec<usize>> = FxMap::default();
        for (i, &p) in pts.iter().enumerate() {
            cells.entry(pack(grid_key(p, inv))).or_default().push(i);
        }
        PointGrid { inv, cells }
    }
    /// Visit every stored index in the 3×3×3 block around `m`.
    fn for_near(&self, m: Vec3, mut f: impl FnMut(usize)) {
        let (ci, cj, ck) = grid_key(m, self.inv);
        for di in -1..=1 {
            for dj in -1..=1 {
                for dk in -1..=1 {
                    if let Some(v) = self.cells.get(&pack((ci + di, cj + dj, ck + dk))) {
                        for &i in v {
                            f(i);
                        }
                    }
                }
            }
        }
    }
}
fn grid_key(p: Vec3, inv: f32) -> (i32, i32, i32) {
    ((p.x * inv).floor() as i32, (p.y * inv).floor() as i32, (p.z * inv).floor() as i32)
}

/// Space-colonization core (Pałubicki §4.1), persistent-field variant. The
/// `markers` are NOT consumed: each call recomputes, per marker, whether it is
/// *occupied* (within ρ of any current `occupiers` = wood) and, if free,
/// associates it to the nearest `bud` that perceives it (within r, a forward
/// cone cos ≥ pcos, and below that bud's reveal ceiling). Returns per bud
/// Some(V) — the normalized sum of directions to the markers it captured — or
/// None.
///
/// Persistence (occupancy vs current wood, not deletion) is what makes the field
/// regenerate: when a plant dies its wood vanishes, reopening its space for
/// neighbours and recruits. Shared by a standalone plant (its own dome) and the
/// ecosystem (one field for the whole stand → genuine competition for space).
/// A bud as seen by space colonization: its tip `pos` and facing `dir`, the
/// `ceiling` (max reachable height) and a crown bound — markers beyond
/// `crown_r` horizontally from `center` are out of this tree's crown, so it
/// fills a species-sized cylinder and competes with neighbours in overlaps
/// rather than racing into open space with a bare limb.
pub(crate) struct BudQuery {
    pub pos: Vec3,
    pub dir: Vec3,
    pub ceiling: f32,
    pub center: Vec3,
    pub crown_r: f32,
}

/// Dense voxel-occupancy grid for the shared-field (`Wood`) case: a marker is
/// occupied iff its cell (at occupancy resolution ρ) holds any wood. A bool
/// grid over the wood bounding box gives O(1) direct-index lookup — far cheaper
/// than hashing ~80k wood points and ~39k marker queries per step. Same
/// predicate as the old hash set, so behaviour is identical.
struct DenseOcc {
    lo: (i32, i32, i32),
    dim: (i32, i32, i32),
    grid: Vec<bool>,
}
impl DenseOcc {
    /// `bounds` (world min/max) lets the caller skip the bounding-box scan when
    /// the extent is already known (the ecosystem's fixed field box) — then the
    /// build is a single pass over wood. Without it, the bbox is derived from
    /// wood in an extra pass.
    fn build(wood: &[Vec3], inv: f32, bounds: Option<(Vec3, Vec3)>) -> Self {
        if wood.is_empty() {
            return DenseOcc { lo: (0, 0, 0), dim: (0, 0, 0), grid: Vec::new() };
        }
        let (lo, hi) = match bounds {
            Some((mn, mx)) => (grid_key(mn, inv), grid_key(mx, inv)),
            None => {
                let (mut lo, mut hi) =
                    ((i32::MAX, i32::MAX, i32::MAX), (i32::MIN, i32::MIN, i32::MIN));
                for &p in wood {
                    let c = grid_key(p, inv);
                    lo = (lo.0.min(c.0), lo.1.min(c.1), lo.2.min(c.2));
                    hi = (hi.0.max(c.0), hi.1.max(c.1), hi.2.max(c.2));
                }
                (lo, hi)
            }
        };
        let dim = (hi.0 - lo.0 + 1, hi.1 - lo.1 + 1, hi.2 - lo.2 + 1);
        let n = dim.0 as usize * dim.1 as usize * dim.2 as usize;
        let mut s = DenseOcc { lo, dim, grid: vec![false; n] };
        for &p in wood {
            if let Some(i) = s.index(grid_key(p, inv)) {
                s.grid[i] = true;
            }
        }
        s
    }
    #[inline]
    fn index(&self, c: (i32, i32, i32)) -> Option<usize> {
        let (x, y, z) = (c.0 - self.lo.0, c.1 - self.lo.1, c.2 - self.lo.2);
        if x < 0 || y < 0 || z < 0 || x >= self.dim.0 || y >= self.dim.1 || z >= self.dim.2 {
            return None;
        }
        Some(x as usize + self.dim.0 as usize * (z as usize + self.dim.2 as usize * y as usize))
    }
    #[inline]
    fn occupied(&self, c: (i32, i32, i32)) -> bool {
        self.index(c).map(|i| self.grid[i]).unwrap_or(false)
    }
}

/// How a marker becomes unavailable.
pub(crate) enum Occ<'a> {
    /// Standalone tree: markers within ρ of a bud are CONSUMED (removed from the
    /// dome). The depletion both advances the frontier and bounds the tree.
    Consume,
    /// Shared stand: occupancy is recomputed each step against current `wood`
    /// (markers persist), so a dead plant's wood vanishing frees its space.
    Wood(&'a [Vec3]),
}

/// Find the nearest bud that *perceives* marker `m` (within perception radius,
/// inside its forward cone, below its reveal ceiling, and within its crown
/// cylinder), returning that bud's index and the unit direction to the marker.
/// Pure (no shared state), so it is safe to run across threads.
fn nearest_bud(
    m: Vec3,
    bgrid: &PointGrid,
    buds: &[BudQuery],
    per2: f32,
    pcos: f32,
) -> Option<(usize, Vec3)> {
    let mut best: Option<usize> = None;
    let mut best_dn = Vec3::ZERO;
    let mut bestd = per2;
    bgrid.for_near(m, |bi| {
        let b = &buds[bi];
        if m.y > b.ceiling {
            return;
        }
        let (hx, hz) = (m.x - b.center.x, m.z - b.center.z);
        if hx * hx + hz * hz > b.crown_r * b.crown_r {
            return; // outside this tree's crown cylinder
        }
        let d = m - b.pos;
        let dist2 = d.length_squared();
        if dist2 > per2 || dist2 < 1e-9 {
            return;
        }
        let dn = d.normalize_or_zero();
        if dn.dot(b.dir) < pcos {
            return;
        }
        if dist2 < bestd {
            bestd = dist2;
            best = Some(bi);
            best_dn = dn;
        }
    });
    best.map(|bi| (bi, best_dn))
}

/// How many contiguous marker chunks the shared-field loop is split across for
/// parallelism. The result is independent of this value: chunks are contiguous
/// and merged in chunk order, so `sum[bi] += dn` replays in the exact sequential
/// marker order — bit-identical regardless of chunk count or thread scheduling.
const COLONIZE_CHUNKS: usize = 16;

pub(crate) fn colonize(
    markers: &mut Vec<Vec3>,
    occ: Occ,
    buds: &[BudQuery],
    occ_r: f32,
    per_r: f32,
    pcos: f32,
    bounds: Option<(Vec3, Vec3)>,
) -> Vec<Option<Vec3>> {
    let bud_pts: Vec<Vec3> = buds.iter().map(|b| b.pos).collect();
    let bgrid = PointGrid::new(&bud_pts, per_r);
    let occ_inv = 1.0 / occ_r.max(0.1);
    let occ2 = occ_r * occ_r;
    let per2 = per_r * per_r;
    // Wood occupancy as a dense voxel grid (cell ≈ ρ): O(1) direct-index lookup.
    let wood_occ = match &occ {
        Occ::Wood(w) => Some(DenseOcc::build(w, occ_inv, bounds)),
        Occ::Consume => None,
    };
    let mut sum = vec![Vec3::ZERO; buds.len()];
    let mut has = vec![false; buds.len()];

    match &occ {
        // Standalone tree: sequential. Occupancy is a bud-distance test, and free
        // markers are kept (the dome depletes). Fast already (one small tree).
        Occ::Consume => {
            let mut kept = Vec::with_capacity(markers.len());
            for &m in markers.iter() {
                let mut occupied = false;
                bgrid.for_near(m, |bi| {
                    let b = &buds[bi];
                    if !occupied && m.y <= b.ceiling && (m - b.pos).length_squared() <= occ2 {
                        occupied = true;
                    }
                });
                if occupied {
                    continue; // reached → consumed (dropped)
                }
                kept.push(m); // free markers persist until reached
                if let Some((bi, dn)) = nearest_bud(m, &bgrid, buds, per2, pcos) {
                    sum[bi] += dn;
                    has[bi] = true;
                }
            }
            *markers = kept;
        }
        // Shared stand: the marker loop is the hot path. Each marker is
        // independent (occupancy is an O(1) dense-grid lookup, perception is the
        // pure `nearest_bud`), so split `markers` into contiguous chunks across
        // threads. Each chunk returns its (bud, direction) assignments in marker
        // order; merging chunks in order replays `sum[bi] += dn` in the exact
        // sequential order → bit-identical, independent of thread count.
        Occ::Wood(_) => {
            let wood = wood_occ.as_ref().unwrap();
            let bgrid = &bgrid; // capture by shared ref (Copy) in the move closures
            let n = markers.len();
            let n_chunks = COLONIZE_CHUNKS
                .min(std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
                .max(1);
            let chunk_size = n.div_ceil(n_chunks).max(1);
            let parts: Vec<Vec<(usize, Vec3)>> = std::thread::scope(|scope| {
                let handles: Vec<_> = markers
                    .chunks(chunk_size)
                    .map(|chunk| {
                        scope.spawn(move || {
                            let mut out: Vec<(usize, Vec3)> = Vec::new();
                            for &m in chunk {
                                if wood.occupied(grid_key(m, occ_inv)) {
                                    continue;
                                }
                                if let Some(p) = nearest_bud(m, bgrid, buds, per2, pcos) {
                                    out.push(p);
                                }
                            }
                            out
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            for part in &parts {
                for &(bi, dn) in part {
                    sum[bi] += dn;
                    has[bi] = true;
                }
            }
        }
    }
    (0..buds.len()).map(|i| has[i].then(|| sum[i].normalize_or_zero())).collect()
}

/// Volume of the intersection (lens) of two spheres.
pub fn sphere_intersection_volume(a: BSphere, b: BSphere) -> f32 {
    let d = (a.centre - b.centre).length();
    let (r1, r2) = (a.radius, b.radius);
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        let r = r1.min(r2);
        return 4.0 / 3.0 * std::f32::consts::PI * r * r * r;
    }
    let sum = r1 + r2;
    std::f32::consts::PI * (sum - d) * (sum - d)
        * (d * d + 2.0 * d * sum - 3.0 * (r1 - r2) * (r1 - r2))
        / (12.0 * d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grow(lambda: f32, steps: u32) -> Plant {
        let mut params = PlantParams::default();
        params.lambda = lambda;
        grow_with(params, steps)
    }

    fn grow_with(params: PlantParams, steps: u32) -> Plant {
        let mut plant = Plant::new(params, Vec3::ZERO);
        for _ in 0..steps {
            plant.step(1.0);
        }
        plant
    }

    // --- basic growth --------------------------------------------------------

    #[test]
    fn seedling_grows_and_branches() {
        let plant = grow(0.52, 12);
        assert!(plant.module_count() > 3, "expected growth, got {}", plant.module_count());
        // Some lateral axes must have formed (order ≥ 1 metamers exist).
        let has_lateral = plant.alive_ids().iter().any(|&id| plant.node(id).order >= 1);
        assert!(has_lateral, "no lateral branches formed");
    }

    #[test]
    fn skeleton_is_nonempty_and_finite() {
        let plant = grow(0.52, 30);
        let segs = plant.skeleton();
        assert!(!segs.is_empty());
        for s in &segs {
            assert!(s.a.is_finite() && s.b.is_finite(), "non-finite segment");
            assert!(s.ra > 0.0 && s.rb > 0.0, "non-positive radius");
            assert!(s.rb >= 0.5 * plant.params.phi - 1e-6, "tip thinner than φ");
        }
        assert!(plant.height() > 0.5, "tree barely grew: {}", plant.height());
    }

    #[test]
    fn growth_stays_bounded() {
        // The resource cap (v_base ≤ v_root_max) must bound the metamer count:
        // per-bud resource falls below 1 as the tree fills, halting growth.
        let plant = grow(0.55, 220);
        let n = plant.module_count();
        assert!(n > 10, "tree too small: {n}");
        assert!(n < 6000, "tree did not stabilize: {n}");
    }

    // (λ's excurrent↔decurrent role is covered by `vigor_split_obeys_apical_
    // control` for the mechanism and `excurrent_species_tower_over_broad_ones`
    // for the emergent contrast. Max *height* is now set by the space-colonization
    // envelope, not λ directly, so there is no λ-only height test.)

    // --- simulation-correctness suite (one test per paper mechanism) ---------

    #[test]
    fn vigor_is_conserved_across_the_split() {
        // Borchert–Honda is a partition: at each internode the resource routed
        // out (to children + active buds) equals the resource that reached it.
        let mut plant = grow(0.55, 40);
        plant.environment(None, None);
        plant.light_pass();
        plant.vigor_pass();
        for id in plant.alive_ids() {
            let n = plant.node(id);
            let order = n.order;
            // Only internodes that actually route resource onward.
            let has_outlet = !n.children.is_empty() || n.terminal_bud || n.lateral_bud;
            if !has_outlet {
                continue;
            }
            let mut out = n.term_resource + n.lat_resource;
            for &c in &n.children {
                out += plant.node(c).vigor;
            }
            // A child of the same order is the main axis; deeper is lateral —
            // both already counted via children vigor above.
            let _ = order;
            let v = n.vigor;
            assert!(
                (v - out).abs() <= 1e-3 * v.max(1.0),
                "internode {id}: vigor {v} != routed-out {out}"
            );
        }
    }

    #[test]
    fn vigor_split_obeys_apical_control() {
        // A seedling internode has exactly one terminal bud (main) and one
        // lateral bud, with equal exposure ⇒ the split is exactly λ:(1−λ).
        fn apical_fraction(lambda: f32) -> f32 {
            let mut params = PlantParams::default();
            params.lambda = lambda;
            let mut plant = Plant::new(params, Vec3::ZERO);
            plant.environment(None, None);
            plant.light_pass();
            plant.vigor_pass();
            let n = plant.node(plant.root);
            n.term_resource / (n.term_resource + n.lat_resource)
        }
        assert!((apical_fraction(0.7) - 0.7).abs() < 0.02, "λ=0.7 leader share");
        assert!((apical_fraction(0.5) - 0.5).abs() < 0.02, "λ=0.5 even split");
        assert!((apical_fraction(0.3) - 0.3).abs() < 0.02, "λ=0.3 lateral-dominant");
    }

    #[test]
    fn light_accumulates_basipetally_to_the_root() {
        // Q_acc(root) must equal the sum of every internode's bud light Q_self.
        let mut plant = grow(0.55, 40);
        plant.environment(None, None);
        plant.light_pass();
        let total: f32 = plant.alive_ids().iter().map(|&id| plant.q_self(id)).sum();
        let root_acc = plant.node(plant.root).q_acc;
        assert!((root_acc - total).abs() < 1e-3, "q_acc(root) {root_acc} != Σ Q_self {total}");
    }

    #[test]
    fn bud_produces_floor_v_metamers_capped_per_step() {
        // The metamer rule (Pałubicki §4.2): a bud with resource v sprouts
        // n = ⌊v⌋ metamers of length l = v/⌊v⌋ (so a shoot's length ∝ vigor),
        // capped per step at MAX_SHOOT.
        let sprout = |v: f32| -> (usize, f32) {
            let params = PlantParams::default();
            let mut plant = Plant::new(params, Vec3::ZERO);
            plant.node_mut(plant.root).terminal_bud = true;
            plant.node_mut(plant.root).lateral_bud = false;
            plant.node_mut(plant.root).term_resource = v;
            let before = plant.module_count();
            plant.grow();
            let added = plant.module_count() - before;
            let len = plant
                .alive_ids()
                .into_iter()
                .find(|&id| id != plant.root)
                .map(|id| plant.node(id).length)
                .unwrap_or(0.0);
            (added, len)
        };
        let il = PlantParams::default().internode_len;
        // Below the cap: exactly ⌊v⌋ metamers, length internode_len·(v/⌊v⌋).
        let (n, len) = sprout(1.6);
        assert_eq!(n, 1, "v=1.6 ⇒ 1 metamer");
        assert!((len - il * 1.6).abs() < 1e-4, "length {len} != {}", il * 1.6);
        // At/over the cap: clamped to MAX_SHOOT metamers.
        let (n, _) = sprout(9.0);
        assert_eq!(n, MAX_SHOOT as usize, "v=9 ⇒ capped at MAX_SHOOT={MAX_SHOOT}");
    }

    #[test]
    fn pipe_model_diameter_is_the_quadratic_sum_of_children() {
        // Eq. 8: an internode's diameter is √(Σ d_child²), floored at φ; a tip
        // sits exactly at φ.
        let plant = grow(0.55, 50);
        let phi = plant.params.phi;
        for id in plant.alive_ids() {
            let n = plant.node(id);
            if n.children.is_empty() {
                assert!((n.diam - phi).abs() < 1e-4, "tip {id} diam {} != φ", n.diam);
            } else {
                let expect = n
                    .children
                    .iter()
                    .map(|&c| plant.node(c).diam.powi(2))
                    .sum::<f32>()
                    .sqrt()
                    .max(phi);
                assert!((n.diam - expect).abs() < 1e-3, "node {id} diam {} != {expect}", n.diam);
            }
        }
    }

    #[test]
    fn pipe_model_thickens_the_trunk() {
        // The basal (trunk) internode must be the thickest — it carries every
        // leaf's pipe.
        let plant = grow(0.6, 70);
        let trunk = plant.node(plant.root).diam;
        let max_d = plant
            .alive_ids()
            .iter()
            .map(|&id| plant.node(id).diam)
            .fold(0.0, f32::max);
        assert!((trunk - max_d).abs() < 1e-4, "trunk {trunk} should be thickest {max_d}");
    }

    #[test]
    fn senescence_drains_root_vigor_past_pmax() {
        // Past p_max the resource ramps to zero (basis for death/gap dynamics).
        // We refresh the marker cloud before each measurement so the *only*
        // thing that can zero the resource is the senescence factor — not marker
        // depletion (a grown tree consumes its envelope and would read 0 anyway).
        fn measure(plant: &mut Plant) -> f32 {
            let (r, h, c) = (
                plant.params.envelope_radius,
                plant.params.envelope_height,
                plant.params.marker_count,
            );
            plant.markers = generate_markers(plant.origin, r, h, c);
            plant.environment(None, None);
            plant.light_pass();
            plant.vigor_pass();
            plant.node(plant.root).vigor
        }
        let mut params = PlantParams::default();
        params.p_max = 20.0;
        let mut plant = Plant::new(params, Vec3::ZERO);
        for _ in 0..6 {
            plant.step(1.0);
        }
        let young = measure(&mut plant);
        plant.age = 3.0 * plant.params.p_max; // well past full senescence (2·p_max)
        let old = measure(&mut plant);
        assert!(young > 0.0, "young root vigor should be positive, got {young}");
        assert!(old < young, "root vigor should fall under senescence: {young} -> {old}");
        assert!(old <= 1e-3, "well past 2·p_max the root should be drained, got {old}");
    }

    #[test]
    fn shedding_drops_starved_branches() {
        // Grow a crown with shedding OFF (so laterals accumulate), then enable
        // an aggressive shed threshold, starve and age the crown, and confirm
        // shed() drops the starved lateral branches (Pałubicki §4.4).
        let params = PlantParams::default(); // shed_ratio 0 ⇒ no shedding while growing
        let mut plant = Plant::new(params, Vec3::ZERO);
        for _ in 0..20 {
            plant.step(1.0);
        }
        let with_laterals = plant.alive_ids().iter().filter(|&&id| plant.node(id).order >= 1).count();
        assert!(with_laterals > 0, "expected laterals to have formed");
        // Drop the crown into shade (light_level 0) and age it, set a shed
        // threshold above that, then shed.
        plant.params.shed_ratio = 0.5;
        for id in plant.alive_ids() {
            plant.node_mut(id).light_level = 0.0;
            plant.node_mut(id).age = 100.0;
        }
        plant.shed();
        let after = plant.alive_ids().iter().filter(|&&id| plant.node(id).order >= 1).count();
        assert!(after < with_laterals, "shedding removed nothing: {with_laterals} -> {after}");
    }
}
