# HANDOFF — Synthetic Sylvaculture

A Rust reproduction of **Makowski et al. 2019, _Synthetic Silviculture: Multi-scale
Modeling of Plant Ecosystems_** (SIGGRAPH / ACM TOG 38(4)), built on the
self-organizing tree model of **Pałubicki et al. 2009, _Self-organizing Tree
Models for Image Synthesis_**. This file is the orientation map. Both papers are
in the repo:

- `Makowski.etal-2019-Synthetic-Silviculture.pdf` — the ecosystem-scale target.
- `selforg.sig2009.pdf` — Pałubicki 2009: the metamer model, extended
  Borchert–Honda vigor, shadow propagation, and **space colonization** (§4.1).

> **What this is now:** an **evolving** self-organizing forest. Individual trees
> grow by the Pałubicki **metamer model** + **space colonization** (one shared
> free-space marker field the whole stand competes for). On top of that, the
> ecosystem **evolves**: there is *no fixed species list* — each plant carries a
> heritable **genome** (`genome.rs`), founders get uniform-random genomes, and
> seeds inherit the parent genome with mutation. Climate is **not** in the genome
> and there is no hardcoded niche; it enters only as physics (two factors —
> warmth & water — stressing different traits), so each biome's community
> *specializes by selection*. Trees compete for light, self-prune, **die of
> carbon starvation or old age** (heritable lifespan → gap churn), reproduce only
> when lit, and a constant **seed rain** carpets the floor. **Negative
> frequency-dependence** (Janzen–Connell) maintains diversity so a climate
> doesn't collapse to one winner. So succession, self-thinning, a layered canopy,
> *and* climate-specialized morphology all **emerge**. The original
> fixed-prototype "branch module" model is gone (`src/prototype.rs` deleted); the
> 7 `species.rs` presets survive only as named archetypes for the single-plant /
> `--tree` inspector, not for the ecosystem.

---

## Quick start

```sh
cd synthetic-sylvaculture

./run.sh --eco           # ECOSYSTEM viewer (the main thing) — size-22 plot
./run.sh                 # single-plant viewer (N cycles species)
cargo run -- --stats     # headless readouts: EVOLUTION trace, 2D specialization, validation
cargo run --release -- --bench   # headless perf benchmark (sim + mesh; see Performance)
cargo test --release     # 31 tests (no GPU); ~18 s
./run.sh --tree 6 --steps 200 --shot t.png            # ONE archetype species, framed solo
./run.sh --tree 0 --steps 160 --bare --shot t.png     # ...skeleton only (branch geometry)
./run.sh --shot e.png --temp 26 --precip 320 --steps 170   # ecosystem frame (+ biome chart)
```

**NixOS:** the GUI must run through `./run.sh` (enters `shell.nix`); plain `cargo
run` can't find the dlopen'd windowing libs. GL is from `/run/opengl-driver/lib`.
**PDFs:** no `pdftoppm` here, so the Read tool can't render pages — extract text
with `nix-shell -p poppler-utils --run "pdftotext file.pdf out.txt"`.

### Viewer controls
- **Ecosystem** (`./run.sh --eco`): Space play/pause · S step · R reseed · F
  foliage · ←/→ temperature · ↑/↓ precipitation · **−/= shrink/grow the plot
  horizontally** · **PageDown/PageUp lower/raise the vertical growth ceiling** ·
  *or click the labelled Whittaker biome chart (top-left)*. Climate changes
  reseed (random founders); the others resize **in place** (the stand is kept).
  The sim runs **unthrottled** (a per-frame time budget, not a fixed cadence) —
  it advances as fast as the frame allows. The biome chart names each region and
  marks the current climate in red; the **floor-light heatmap (top-right)** shows
  how much light reaches the floor across the plot (dark = shaded, bright = sun).
  Mouse orbits/zooms.
- **Single plant** (`./run.sh`): Space/S/R · N cycle archetype species · ←/→
  apical control λ · ↑/↓ growth rate · F foliage. (Inspector for the `species.rs`
  presets — *not* the evolving ecosystem.)

### Self-verifying renders
`--shot`/`--tree` render off-screen to a PNG you then open. **A "Segmentation
fault" from these is NOT a failed render** — they `std::process::exit(0)` after
saving to skip a Wayland-teardown crash; the PNG is already written. Always judge
by the PNG. If renders genuinely OOM-crash, check `nvidia-smi` (full GPU VRAM
looks like a geometry bug).

---

## File map

| File | What |
|---|---|
| `src/plant.rs` | The core. `Plant` (metamers + terminal/lateral/**relay** buds; `live` counter + min-heap free-list back `module_count`/`alloc`), the growth cycle, **sympodial relay** (`relay_bud`, `relay_direction`), **age-dependent apical control** (relaxed λ in `vigor_pass`), **space-responsive crown** (`maturity`, `crown_radius`, expanding `reveal_ceiling`), **basitony** (basal laterals up-right into a multi-stem bush, in `lateral_direction`), the `colonize`/`Occ`/`BudQuery`/`PointGrid`/`DenseOcc` space-colonization core, `FxHasher`/`pack` voxel hashing, `hash01` (deterministic bud-fate), self-shadow, shedding, **memoryful** pipe-model diameters (never shrink), **biomechanical breaking** (`break_overstressed`/`bending_stress`: cantilever stress > `BREAK_STRESS` snaps a branch), `health` (crown-tip carbon), `root_vigor`, geometry queries. |
| `src/genome.rs` | **The evolving genome.** **19** heritable traits (morphology + life-history incl. `lifespan`, `apical_relax`, and `basitony` = shrub habit); `random` (founders), `mutated` (heritable seeds), `to_params` (derives marker/module budget from the *expanded* crown volume), `niche` (behaviour descriptor for frequency-dependence), `leaf_rgb`/`foliage_style` (colour *and* broad↔needle leaf shape *from* the genome → watch a biome converge), `bark_rgb`. |
| `src/species.rs` | 7 plant-type **archetype presets** `preset(λ,D,gp,v_root_max,g2,s_tol,φ,env_h,env_r)` — used ONLY by the single-plant/`--tree` inspector + the morphology tests. The ecosystem evolves genomes, not these. (Climate-niche fields are dead, `#[allow(dead_code)]`.) |
| `src/ecosystem.rs` | `Ecosystem` (now **genome-based, evolving**): shared marker field (`regenerate_field`, `set_size`/`set_field_height` resize), `ShadowGrid`, `Climate::warmth/water/productivity`, `survival_bar` (2D-climate carbon cost), `cull_dead` (starvation + senescence + **Janzen–Connell** `similar_crowding`), `seed` (inherit+mutate + **seed rain** + **clonal/vegetative spread** for basitonic shrubs, vigor-scaled maturity), `mean_traits`/`trait_std`/`established_count`/`stratum_counts`, `step_timed`, parallel grow + mesh gather. |
| `src/mesh.rs` | Skeleton → generalized-cylinder mesh; foliage blades (`leaf_blade` morphs broad↔needle per `LeafStyle{rgb,needle}`); **parallel in-place** per-plant-coloured forest mesh (`balanced_ranges`/`carve_mut`/`uninit_vec` → prefix-sum slice fill, no concat). |
| `src/overlay.rs` | Clickable Whittaker biome chart (top-left) **with biome name labels** (self-contained 5×7 `glyph` bitmap font + `push_text`); `screen_to_climate`; the **floor-light heatmap** (top-right, `build_floor_light`/`heatmap_rect`/`light_color` — a shade→sun heat ramp of how much light reaches the floor). |
| `src/main.rs` | Viewers (`run`, `run_ecosystem` with resize keys + unthrottled stepping), `run_tree_shot` (`--tree [--bare]`), `run_shot`, `run_stats` (EVOLUTION trace + 2D specialization + validation), `run_bench`. |

---

## The model

A plant is a tree of **metamers** (`Internode` + axillary lateral bud; an axis
tip carries a terminal bud). `Plant::new(params, origin)`. Each step:

1. **environment — space colonization (§4.1).** Buds compete for free-space
   markers. Occupancy modes (`Occ`):
   - **`Consume`** (standalone tree, `--tree`/single viewer): the plant depletes
     its own private marker dome; depletion advances the frontier and bounds it.
   - **`Wood`** (ecosystem): one **shared, persistent** field; occupancy is
     recomputed each step against current wood (a voxel set), so a dead plant's
     space reopens for neighbours and recruits.
   Each bud (`BudQuery`) has a reveal **ceiling** and a **crown-radius** bound —
   a cylinder it competes within (no bare limbs racing into open space). The
   cylinder is **space-responsive** (`crown_radius()` / `reveal_ceiling()`): the
   genome envelope is the *young* crown, and the potential crown grows with
   maturity (age/p_max) toward ~1.8× radius × 1.3× height — but the tree only
   *fills* it where free markers exist, so a gap/open tree spreads and rises into
   old age, a crowded one stays bounded, and a survivor expands into a dead
   neighbour's freed space (this is what gives self-thinning ≈ −1.25 and lets old
   crowns spread). A free marker goes to the nearest perceiving bud (within r, a
   forward cone); the bud's growth direction `V` is the sum of marker directions.
   `Q` = space-presence × global-shadow light `g`.
2. **light pass** — `Q` accumulates basipetally → `Q_acc`.
3. **vigor pass** — resource `v = α·Q_base` flows acropetally, split by extended
   Borchert–Honda: `vm = v·λQm/(λQm+(1−λ)Ql)`, `vl = …`.
4. **carbon balance** — `health` = EMA of the **crown-TIP raw light** (carbon
   income from the foliage, *not* floored by shade tolerance — that was a
   degenerate "free health" exploit). Drives mortality in the ecosystem.
5. **bud fate** — a bud with resource `v` sprouts `n = ⌊v⌋` metamers of length
   `v/n` (capped at `MAX_SHOOT`/step). Shoot length ∝ vigor. A continuing shoot's
   heading is the **weighted sum of three vectors** (Pałubicki §4.2): the
   *default orientation* (the parent axis heading, weight 1 — the axis stiffness
   that keeps a bole straight), the optimal growth direction `V` (weight `ξ`),
   and tropism (weight `η`, the `Vec3::Y` pull in `sprout`). Pure `V` (dropping
   the default term) makes axes wander/wiggle like worms. **Determinacy →
   monopodial vs sympodial:** high D → the terminal bud keeps continuing (single
   straight leader); low D → the terminal "flowers"/stops and a **relay bud**
   (separate from the lateral bud, so the tip still side-branches — no
   starvation) takes over as a near-axial leader → a gently zig-zag sympodial,
   bushier decurrent crown. The relay draws the apical λ share. **Basitony →
   multi-stemmed bush:** near the ground a basitonic genome turns its lowest
   laterals upward (toward vertical) into a clump of co-equal *stems* instead of
   angled side branches, so high `basitony` + low λ + a short envelope grows a
   shrub, while `basitony` 0 stays a single trunk (a tree). Only the basal
   lateral *direction* is changed — the vigor routing is untouched.
6. **shedding** (§4.4) — a lateral branch whose mean light is below `shed_ratio`
   is dropped → clean boles under shade (shade-tolerant species keep theirs).
7. **diameters** — pipe model `d = √(Σ d_child²)`, φ at the tips (Eq. 8), so
   **trunk diameter ∝ √(leaf count)**. With a **memory** (§4.4): diameter is
   monotonic non-decreasing, so a cleaned bole keeps the girth it grew when it
   still carried the now-shed crown (width isn't lost when branches are shed).
8. **breaking** (`break_overstressed`, an extension *beyond* the papers) — each
   internode is a **cantilever**: bending stress at its base ≈ (distal subtree
   weight × horizontal lever-arm) / (section modulus ∝ `diam³`). Past
   `BREAK_STRESS` the branch **snaps off** (mechanical failure, distinct from
   shedding). Vertical leaders have ~zero lever arm (safe); long horizontal
   cantilevers break once they over-extend past what their pipe-model thickness
   supports. Trunk (order 0) is exempt. NB the pipe model *already* keeps healthy
   stress roughly uniform (~200–370, the uniform-stress hypothesis), so breaking
   only trims the ecosystem's over-extended tail (leaners / gap-reaching crowns,
   which run to ~2400) — after it, the stand's max stress is capped at the
   threshold, i.e. every standing branch is physically supportable.

**Apical control λ ≈ 0.5** spans excurrent↔decurrent (Pałubicki Fig. 7), and is
**age-dependent**: the effective λ relaxes from the genome `lambda` toward
`lambda − apical_relax` over the plant's life (Pałubicki Fig. 10/11; Makowski
λ→λ_mature), so a tree can be excurrent young and decurrent old. Crown size is
set by the **marker-cloud envelope**, which is **space-responsive** (the genome
value is the young crown; it grows with maturity — see step 1), not by λ.
Determinacy `D` does double duty (coherently): branch *angle* (high D narrow, low
D wide) **and** monopodial↔sympodial (see step 5).

### Light & sun (the shadow model — and why it's vertical)

The sun is **vertical** (light from above), via Pałubicki's downward
shadow-propagation grid (`ShadowGrid`, ecosystem) / `self_shadow` (standalone):
each occupied voxel adds a downward pyramidal penumbra `Δs = a·b^−q` (`a=1`,
`b=2`, `qmax=6`), and a point's light is `Q = max(C − s + a, 0)/C` (`C=11`). Read
in reverse this **already integrates occlusion over an upward ~45° cone** — i.e.
a crude zenith-weighted hemisphere of diffuse sky light (which is why the
floor-light heatmap reads like ambient occlusion). Neither paper has a moving or
angled sun; climate is *annual-averaged*.

- **Occlusion is partial and cumulative, not per-branch opaque.** The first unit
  of shadow is free (`s ≤ a=1` ⇒ `Q = 1`), and one module adds only `a·b^−q`
  (≤1), so a *single* branch barely dims the light; blackout needs `s ≥ C+a`
  (=12) ≈ many stacked modules. So per-branch the canopy is already very *leaky*
  — the dark floor under a dense crown is the sum of dozens of modules, not any
  one being solid. (Real forest floors get only ~1–5% of full sun, so "dark" is
  realistic.) **`C` is the canopy-leakiness knob** (higher ⇒ a denser column is
  needed to darken the floor ⇒ leakier); it was raised 8→11 for a brighter,
  dapplier floor. It is **load-bearing** (it also sets the strength of light
  *competition* — self-thinning, succession), but the floor-brightening is the
  only *robust* effect: the apparent shifts in the shrub stratum and self-thinning
  slope across `C`∈{8,9,11} turned out to be within **single-seed noise** (the
  understory got *more* shrubs at 9, fewer at 11 — not monotonic), so `C` is an
  aesthetic dial, not a tuned ecological optimum. Validation held at 11. Dial it
  freely, but judge any ecological claim multi-seed at settled horizons.
- **Hemispherical sun — tried & reverted.** An explicit "all angles, most at top"
  model (a strong zenith deposit + a ring of 4 oblique deposits, weights summing
  to 1 so uniform-canopy darkness is preserved) was built and measured. Even
  tuned gentle (zenith 0.80 / obliques 0.05) it was a **net loss**: the oblique
  side-light biased branch vigor toward gaps (the banana-arc pathology returned
  at full strength), and at *settled* horizons it **halved the climate-
  specialization contrast** (tropical-vs-boreal crown breadth 3.5→1.6) and
  **suppressed the tropical thicket understory** (basitony 0.57→0.05). Same
  side-lighting mechanism drove both the prettier softer floor shadows *and* the
  harm → no sweet spot. The vertical model is the right fit; see the `ShadowGrid`
  doc comment. **General lesson:** the light model is the load-bearing wall of the
  sim — even a *scale-neutral* change to its angular structure re-selects the
  evolved equilibria. Change it only with a settled-horizon `--stats` + a revert
  plan.

### Ecosystem (Sec. 6) — now EVOLUTIONARY, no fixed species

Each plant carries a `Genome` (`genome.rs`); the `Ecosystem` stores one per plant
(parallel to `plants`). There is **no species list and no climate niche** — biome
specialization emerges from selection:

- **Founders** get uniform-random genomes; **seeds inherit the parent genome +
  small Gaussian mutation** (`mutation_rate`). Heritability is what lets selection
  accumulate.
- **Climate = two physical factors on different traits** (the 2D Whittaker axes,
  `Climate::warmth`/`water`): **water** limits affordable crown *volume* (dry ⇒
  small/sparse), **warmth** drives growth *rate* and flips a crown-*breadth* cost
  (cold ⇒ narrow/conical, warm+wet ⇒ broad). `survival_bar` folds these into the
  carbon cost — a big/broad crown only breaks even where the climate affords it.
  A liveability floor (`MAINT_BASE/productivity`) keeps the harsh corners barren.
  ⇒ cold-wet → narrow conifer-like, warm-wet → broad broadleaf, dry → sparse.
- **Carbon-starvation mortality** (`cull_dead`): an established plant
  (`age > CARBON_ESTABLISH`) dies when `health` < its `survival_bar`. Shade
  tolerance *lowers* the bar (subsist in shade) but *costs growth* — a real
  tradeoff, not free health.
- **Lifespan** is a heritable genome trait → senescence death → **gap churn**, so
  even canopy winners die and selection keeps compounding (a stand of immortals
  freezes). Death opens space; `Wood`-mode occupancy reopens it for recruits.
- **Reproduction needs LIGHT, not just survival** (`health ≥ FLOWER_LIGHT`,
  tolerance-independent) — a shaded understory survives but can't breed until it
  reaches a gap. (Without this, small shade-tolerant plants out-breed canopy
  trees in the dark → the whole stand collapses to a lawn of sprouts.) Flowering
  age is **vigor-scaled** (Makowski F_eff): vigorous plants mature sooner.
- **Seed rain**: every step the floor is carpeted with establishment attempts —
  mostly from the proven reproductive pool, plus a small random-immigrant
  fraction — so gaps fill instantly; most seedlings starve, gap ones take hold.
- **Clonal / vegetative spread** (the shrub strategy): a **basitonic**,
  established plant puts up a near-clone **sucker** nearby *without needing to
  flower* (real clonal shrubs: hazel, sumac, aspen), so a basitonic shrub can
  persist and form a **thicket** in shade where it can't reach flowering light —
  bypassing the tall-biased seed pool. Keyed on `basitony` only (not light or
  tolerance), so it gives the shrub guild a foothold without perturbing the
  flowering rule. This is what fills the low **understory** layer (A/B: with it
  the stand is a layered forest, without it a sparse scatter of lone trees).
- **Diversity via Janzen–Connell** (`similar_crowding`): a plant crowded by
  *near and niche-similar* neighbours suffers extra mortality (negative
  frequency-dependence = the ecological twin of GA fitness-sharing), so rare
  strategies are protected and a climate doesn't collapse onto one winner.
- **Appearance is derived from the genome** — slenderness → leaf hue *and*
  broad↔needle leaf **shape** (`foliage_style`: tall-narrow → needle sprays =
  conifer, short-broad → wide diamonds = broadleaf), tolerance → brightness — so
  a specializing biome is *seen* to converge in colour *and* foliage form.
  Measure the *established* cohort (`mean_traits`/`trait_std`/`established_count`,
  `stratum_counts` for the shrub/tree split), not the transient seedling carpet.

---

## What's done (recent first)

**Evolutionary phase (the current model — replaced the fixed-species ecosystem):**
random-genome founders + heritable mutation; mechanistic **2D climate** (warmth &
water on different traits → Whittaker specialization); **carbon model** reworked
(crown-tip raw light, intrinsic size cost, tolerance↔growth tradeoff, reproduce
only when lit); heritable **lifespan** → gap churn; **seed rain**; **Janzen–
Connell** frequency-dependence for diversity; genome-derived colour; **sympodial
branching** via a relay bud; **vigor-scaled maturity**; **age-dependent apical
control** (λ relaxes young→old); **space-responsive crowns** (the elastic envelope
that lifted self-thinning to ≈ −1.25 and lets old crowns spread); **memoryful
pipe-model diameters** (never shrink on shedding — a paper-accuracy fix);
**emergent foliage morphology** (broad↔needle leaf shape from genome slenderness —
conifers grow needle sprays, broadleaves wide leaves); **bushes** (a heritable
`basitony` trait — the 19th — that up-rights basal laterals into a multi-stemmed
shrub clump, plus a lowered height floor so short shrubs are reachable); **clonal
/ vegetative spread** (basitonic plants sucker near-clones without flowering →
a self-sustaining understory thicket layer, bypassing the tall-biased seed pool);
**biomechanical breaking** (over-extended cantilever branches snap when the pipe-
model thickness can't bear the load — caps the over-stressed tail); a **leakier
canopy** (`ShadowGrid.c` 8→11 → brighter dappled floor). Viewer:
in-place **resize** keys, **unthrottled** stepping, **biome labels**. The
branch-shape fix (default-orientation term, killed the wiggle) and the perf work
(below) predate this. Reverted as destabilizing/infeasible in this model: ongoing
per-segment tropism (weeps long branches); a tolerance→flowering-light discount
meant to sustain an understory shrub layer (it collapsed a marginal biome — see
Known limitations); an explicit **hemispherical sun** (zenith + oblique deposits,
"light from all angles, most at top") — see the **Light & sun** note below.

**Earlier (the self-organizing rewrite + perf), recent first:**

- `4553e49` **Canopy scale-up** — taller/thicker canopy species (tropical/conifer/
  poplar, envelopes 28–30, `max_modules` 1600–2500), larger plot (size 22),
  raised field height (34) → tall thick trees and a layered stand.
- `063f3f2` **Carbon-balance mortality** — competition-driven death (mean-foliage-
  light health) → proper pioneer→climax succession.
- `e5eec64` **Shared marker field** — stand-scale space colonization; genuine
  competition; self-thinning steepened −1.05 → −1.37.
- `f8637e4` **Self-shading + validation** — per-plant shadow for isolated trees;
  `--stats` pipe-model & self-thinning fits.
- `25c55df` **Shedding** — clean boles (§4.4).
- `c2d9e0d` **Gradual growth** — the rising reveal ceiling.
- `f06896b` **Space colonization** — bounds & shapes growth (fixed the whip).
- `b984f04` **Metamer-model rewrite** — replaced the fixed-prototype module model;
  deleted `prototype.rs`.
- `547aed5` **Correctness suite** — one test per paper equation.
- earlier: the retired module model (M1–M5) — ecosystem/mesh/overlay/shot
  infrastructure carried forward; only the plant-growth core was replaced.

**31 tests pass.** The `plant.rs` mechanism suite verifies the equations directly
(BH split, `n=⌊v⌋`, basipetal light, pipe model √Σd², shedding, senescence, vigor
conservation, growth bounded, **cantilever breaks / vertical survives**).
`ecosystem.rs` tests cover the emergent properties (2D climate specialization —
multi-seed since the established cohort is small/noisy; shadowing suppresses
biomass; canopy stays upright; resize culls out-of-bounds; floor-light sane).
`species.rs`/`overlay.rs` tests are non-degeneracy + glyph-coverage sanity.

---

## Quantitative validation (`cargo run -- --stats`)

The headline payoff of getting the mechanisms right — the model agrees with laws
it was never told:

- **Pipe-model allometry** (Eq. 8): trunk diameter vs leaf count → log-log slope
  **≈ 0.51** (predicted 0.50; diameter ∝ √leaves). Holds.
- **Self-thinning** (Yoda's −3/2 law): a dense even-aged **monoculture**
  (`Ecosystem::monoculture`, seeding off) thins while mean biomass rises → slope
  **≈ −1.25** (ideal −1.5). With **space-responsive crowns** (below) survivors
  expand into freed space as the stand thins, so mean biomass keeps climbing
  (~0.04 → ~1.2) instead of plateauing — the right −3/2 behaviour. The residual to
  −1.5 is the early establishment crash (mass seedling die-off) + discrete
  sampling, not the old fixed-envelope cap.

---

## Performance (`cargo run --release -- --bench`)

`--bench` is a headless, deterministic benchmark (no GPU/mesh-upload, no new
deps): it grows a worst-case tropical stand and reports per-phase ms/step for
the sim (centres / colonize / shadow / grow / cull), a CPU mesh-build section
(the per-frame render cost, GPU upload excluded), and a single-plant figure.
`Ecosystem::step_timed` returns the per-phase breakdown.

A 9-round pass took the worst case (tropical, 40→170 plants, ~80k modules) from
**~25 s → ~3.6 s** wall for 170 sim steps (**~7×**; ~149 → ~21 ms/step), and the
**per-frame mesh rebuild from ~148 ms → ~13 ms** (**~11×**, ~2.2M verts). A heavy
interactive frame's CPU work (sim + mesh) went from ~300 ms to ~35 ms; the test
suite from ~231 s to ~3 s (it is ~18 s today — the multi-seed evolution test was
added since). The single biggest fix was an accidental **O(n²)** —
`module_count()` (then O(live)) called inside `grow()`'s per-bud loop, plus a
linear free-slot scan in `alloc()`.

What changed, in order (every step kept the stand **bit-identical** — final
170 plants / 79885 modules — or, where f32 summation order changed, the stand
came out identical and validation slopes held):

1. **O(1) `module_count`** (a `live` counter) + **min-heap free-list `alloc`**
   (reuses the lowest free slot, matching the old scan) — killed the O(n²).
2. **FxHash** (rotate-xor-multiply) over **packed u64 voxel keys** for the
   space-colonization grid / occupancy / self-shadow. NB: a plain multiplicative
   hash on a packed key clusters badly (bucket index depends only on the lowest
   coord); a **splitmix64 finalizer** fixes it — that was the whole win.
3. **Shadow deposit**: branch-free clamped inner loop + **bin by voxel** (one
   weighted pyramid per occupied cell, not one per module).
4. **Dense bool occupancy grid** for the shared field (direct index, no hashing;
   bounded to the fixed field box so it builds in one pass).
5. **Parallel colonize** (the marker loop) and **parallel grow** (over plants),
   via `std::thread::scope` (no deps). Both stay **bit-identical**: colonize uses
   contiguous marker chunks merged in order (so `sum[bi] += dn` replays in
   sequential order); grow mutates disjoint plants in place.
6. **Parallel forest mesh build**: per-chunk vertex counts → prefix-sum offsets
   → one allocation carved into disjoint slices → each work-balanced contiguous
   chunk fills its slice on a thread. No concat copy; `uninit_vec`
   (`with_capacity` + `set_len`, sound because the fill writes every plain-data
   element) skips zeroing ~38 MB. Plus a **parallel per-plant gather**
   (`skeleton`/`leaves`). Vertex order is preserved → bit-identical mesh.

Determinism note: the parallel paths are reproducible because chunks are
contiguous and merged/laid-out in order, independent of thread count or
scheduling. (NB the "bit-identical 79885 modules" above is the *old fixed-species*
worst case the perf work was measured against; the current evolving stand is
lighter — ~10k modules — and is reproducible per seed, but the model changes
since then mean it is not byte-identical to that historical number.)

---

## Tuning harness

`--stats` prints: an **EVOLUTION** trace (mean genome over a long temperate run —
watch it converge / churn), **2D specialization** across the four Whittaker
corners (evolved means + diversity σ + established + shrub/tree strata),
per-archetype morphology, and the validation fits. Tune by reading the numbers
**and** a `--tree`/`--shot` PNG.

> **Timescale (important):** a stand takes **~2000–3000 steps to actually settle
> into its climate-adapted niche** — earlier readings are *transient* and can
> mislead (this is why this session's short single-seed runs gave jumpy
> desert/shrub counts). `--stats` therefore runs the EVOLUTION trace to 3000 and
> each 2D corner to 2400 steps (so the means are settled), which makes a full
> `--stats` ~**90 s** (the `cargo test` suite is separate, still ~18 s). When you
> change a constant, judge it at a settled horizon, not at a few hundred steps.

What to tune:
- **Genome trait ranges** (`genome.rs` `RANGES`) — the evolvable bounds for all
  **19** traits (λ, determinacy, α, gp, v_root_max, g2, tropism_up, ξ, φ,
  shade_tolerance, shed_ratio, env_h *(floor 1.5 m — shrubs)*, env_r,
  flowering_age, seed_radius, seed_freq, lifespan, **apical_relax**,
  **basitony** *(shrub habit)*). Founders draw uniform here; mutation clamps
  here. (Append new traits LAST — the `--stats` key + the [f32;19] aggregations
  index by position.)
- **Ecosystem constants** (`ecosystem.rs`): the **2D-climate carbon** consts
  `MAINT_BASE`/`MAINT_VOL`/`MAINT_BREADTH`/`MAINT_FULL_VOL`; `FLOWER_LIGHT`
  (reproduce-only-when-lit threshold); the **Janzen–Connell** `JC_RADIUS`/
  `JC_NICHE_SIGMA`/`JC_MAX`/`JC_HALF` (diversity strength); `SEED_RAIN`/
  `IMMIGRANT_FRAC`; the **clonal-spread** knobs `CLONE_FREQ`/`CLONE_RADIUS`/
  `CLONE_BASITONY_MIN` (understory thicket density — too high risks a clonal
  takeover; A/B against `CLONE_FREQ=0`); `CARBON_ESTABLISH`; `max_plants`;
  `mutation_rate`.
- **Biomechanics** (`plant.rs` consts): `BREAK_STRESS` (cantilever stress at
  which a branch snaps — above the healthy ~200–370 band so only over-extended
  branches break; lower = more breakage), `LEAF_MASS` (tip foliage weight). Set
  `BREAK_STRESS` ≥ 1e9 to disable (A/B). Read the per-species `max_stress` +
  grown-stand stress distribution in `--stats`.
- **Global plant feel** (`PlantParams`/`plant.rs` consts): `MAX_SHOOT`, `ξ`
  (axis-stiffness, default 0.25 — low = straight/stiff, high = wandering),
  `CROWN_EXPAND_R`/`CROWN_EXPAND_H` (how far the space-responsive crown grows with
  age), `FIELD_DENSITY`, `MAX_FIELD_HEIGHT` (now just the default field ceiling),
  `OCC_R`/`PER_R`/`PER_COS`.
- **Archetype presets** (`species.rs`) — only affect the single-plant/`--tree`
  inspector and the morphology tests, *not* the evolving ecosystem.

> **Rigor caveat (be honest with yourself):** most of these constants were tuned
> by eye against *single* `--stats`/`--tree` runs, not distributions. The
> emergent system has many interacting tuned parameters; specific properties were
> checked multi-seed (the 2D-specialization test), but the whole is not
> systematically robustness-tested. When you change a constant, re-run `--stats`
> and re-check the validation fits — they *do* drift (self-thinning slid from
> ≈−1.5 to −0.95 across the model changes before anyone noticed).

---

## Known limitations & gotchas

1. **Performance** — healthy. The evolving stand is *lighter* than the old
   hand-tuned giants the perf section benchmarks (~10k modules vs ~80k), so a
   heavy frame is now ~**8–11 ms sim** + ~**7–9 ms** CPU mesh build. The test
   suite is **~18 s** (most of it the multi-seed evolution test; the mechanism
   tests are a few seconds). The remaining interactive cost is the **GPU upload**
   — `Mesh::new` re-uploads all verts every dirty frame; `--bench` does NOT
   measure that → the **LOD / instancing** future item. Still: **don't run
   multiple `cargo` invocations at once** (build-lock); use `run_in_background`.
   Bench on `--release`, low-load box (a busy machine inflates all phases).
2. **`--shot`/`--tree` "Segmentation fault" ≠ failed render** (exits after writing
   the PNG to skip the Wayland teardown). Always Read the PNG. Check `nvidia-smi`
   if renders OOM.
3. **Single-tree lean / `--tree` asymmetry** — in `Consume` mode (the standalone
   inspector) a tree consumes its private marker dome asymmetrically, so isolated
   trees come out lopsided/windswept (decurrent ones lean ~0.3–0.5). This makes
   `--tree` a *somewhat unreliable* tuning instrument — judge species character,
   not perfect symmetry. In the ecosystem (`Wood` mode, all-sides competition)
   trees are more balanced.
4. **Self-thinning is ≈ −1.25, not −1.5** — the residual is the early
   establishment crash + discrete sampling, not the old fixed-envelope cap
   (crowns are now space-responsive; see Validation).
5. **Parameter fragility** — see the Rigor caveat above. Many eye-tuned constants;
   the emergent ecosystem can shift with any of them. Re-run `--stats` after edits.
6. **Naming debt**: `term_resource` now also carries the relay's main share;
   `MAX_FIELD_HEIGHT` is now just the default (field height is a runtime field).
   `determinacy` does double duty (branch angle + sympodial).
7. **Species presets are adapted, not transcribed** from Tab. 4 (units differ).
8. The **viewers** still crash on the Wayland teardown at window close (cosmetic;
   only the scripted `--shot`/`--tree` paths were fixed).

---

## What remains / future work (priority order)

- **GPU upload / LOD / instancing** — the remaining interactive cost now that
  sim + CPU mesh build are fast. `Mesh::new` re-uploads ~2.2M verts/frame; cut it
  by (a) **vertex reduction / LOD** (fewer trunk sides for thin branches, cheaper
  foliage — cuts both CPU build and upload), (b) **instancing** (one leaf/segment
  proto, per-instance transforms), or (c) **rebuild/upload only when changed**
  (every N steps, or only dirty plants). Instrument `run_ecosystem` to measure
  real frame time first.
- **Terrain + elevation lapse rate** `T(h)=T(0)+γh` → **treelines** (Makowski
  Sec. 6.4). The biggest remaining paper feature and a clean fit (climate is
  already mechanistic). Plus a **soil/blocked map** (exclude water/rock/roads).
- **Robustness pass** — sweep the eye-tuned constants across seeds; turn the
  ad-hoc tuning into measured distributions (see the Rigor caveat).
- **A distinct SHORT understory shrub stratum.** Mostly addressed: **clonal /
  vegetative spread** (basitonic plants sucker thickets without flowering) now
  fills a self-sustaining understory layer (A/B: layered forest vs sparse lone
  trees). What remains rare is a distinct *short* (env_h<4) stratum — the
  seed-rain pool is built from reproductive (lit, *tall*) plants, so it's
  tall-biased and short genomes arrive only as rare immigrants; clonal spread
  bootstraps the short-basitonic ones that do establish, but a broad short-shrub
  carpet doesn't form. **The seed-pool tall-bias that starves the short carpet is
  LOAD-BEARING, not a defect** — it is what keeps the canopy strategy winning
  over the shade-subsister. Three things were tried and **reverted**: a
  tolerance-only flowering-light discount (collapsed the marginal desert); a
  shortness+tolerance-gated discount (desert-safe but inert — nothing short
  enough evolves to use it); and a graded fecundity-∝-light pool (de-biased the
  pool but reopened the sprout collapse *in aggregate* — many shaded plants each
  breeding a trickle drifts the pool to small/tolerant over a long run). LESSON:
  any aggregate-significant *shade reproduction* is the sprout collapse, slowed
  down. The only safe way to populate the understory is **clonal spread** (keyed
  on the structural `basitony` habit, not on shade reproduction) — which is the
  thicket layer we have. A true short-shrub *carpet* may simply not be reachable
  without a different recruitment model (e.g. a real seed bank with its own
  bounded dynamics), and isn't worth chasing through the flowering rule.
- **Other paper gaps** (from the two-paper audit): **disturbance** (fire/wind →
  succession reset), a **grass/forb understory** layer, **flowering changes λ/D**
  (mature-form change, e.g. Baobab), the Pałubicki **priority** bud-fate model.
- **Richer foliage / more biome coverage** — textured leaf-shaped quads, grass.
- **Window-close teardown** — clean viewer exit (same `process::exit` trick).

---

## Conventions
- Verification here = `cargo test --release` (CPU, ~18 s, 31 tests) + `cargo run -- --stats`
  (CPU) + a `--tree`/`--shot` PNG you actually open; `--bench` for perf. Commit
  freely; small commits preferred.
- See `../CLAUDE.md` for workspace context (mostly Verus-specific; this subproject
  is plain Rust + three-d, no formal verification).
