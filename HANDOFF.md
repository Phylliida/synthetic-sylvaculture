# HANDOFF — Synthetic Sylvaculture

A Rust reproduction of **Makowski et al. 2019, _Synthetic Silviculture: Multi-scale
Modeling of Plant Ecosystems_** (SIGGRAPH / ACM TOG 38(4)), built on the
self-organizing tree model of **Pałubicki et al. 2009, _Self-organizing Tree
Models for Image Synthesis_**. This file is the orientation map. Both papers are
in the repo:

- `Makowski.etal-2019-Synthetic-Silviculture.pdf` — the ecosystem-scale target.
- `selforg.sig2009.pdf` — Pałubicki 2009: the metamer model, extended
  Borchert–Honda vigor, shadow propagation, and **space colonization** (§4.1).

> **What this is now:** a self-organizing forest. Individual trees grow by the
> Pałubicki **metamer model**; the whole stand competes for one **shared
> free-space marker field** (space colonization); trees are bounded and shaped by
> that competition + light, self-prune clean boles, and **die of carbon
> starvation** when overtopped — so succession (pioneer→climax), self-thinning,
> and a layered canopy all *emerge*. The original fixed-prototype "branch module"
> model is gone (`src/prototype.rs` deleted). Nearly every behaviour is emergent,
> tuned by a small set of per-species parameters, not scripted.

---

## Quick start

```sh
cd synthetic-sylvaculture

./run.sh --eco           # ECOSYSTEM viewer (the main thing) — size-22 plot
./run.sh                 # single-plant viewer (N cycles species)
cargo run -- --stats     # headless readouts incl. quantitative validation
cargo run --release -- --bench   # headless perf benchmark (sim + mesh; see Performance)
cargo test --release     # 25 tests (no GPU); ~3 s (was ~4 min before the perf work)
./run.sh --tree 6 --steps 200 --shot t.png            # ONE species, framed solo
./run.sh --tree 0 --steps 160 --bare --shot t.png     # ...skeleton only (branch geometry)
./run.sh --shot e.png --temp 26 --precip 320 --steps 170   # ecosystem frame
```

**NixOS:** the GUI must run through `./run.sh` (enters `shell.nix`); plain `cargo
run` can't find the dlopen'd windowing libs. GL is from `/run/opengl-driver/lib`.
**PDFs:** no `pdftoppm` here, so the Read tool can't render pages — extract text
with `nix-shell -p poppler-utils --run "pdftotext file.pdf out.txt"`.

### Viewer controls
- **Ecosystem** (`./run.sh --eco`): Space play/pause · S step · R reseed · F
  foliage · ←/→ temperature, ↑/↓ precipitation, *or click the Whittaker biome
  triangle (top-left)*. Try temperate (poplar emergents over understory) vs
  tropical (closed canopy). Mouse orbits/zooms.
- **Single plant** (`./run.sh`): Space/S/R · N cycle species · ←/→ apical
  control λ · ↑/↓ growth rate · F foliage.

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
| `src/plant.rs` | The core. `Plant` (metamers + buds; a `live` counter + min-heap free-list back `module_count`/`alloc`), the growth cycle, the `colonize` (parallel marker loop)/`Occ`/`BudQuery`/`PointGrid`/`DenseOcc` space-colonization core, the `FxHasher`/`pack` fast voxel hashing, self-shadow, shedding, pipe-model diameters, `health` (carbon balance), and geometry queries. |
| `src/species.rs` | 7 plant-type presets `preset(λ, D, gp, v_root_max, g2, s_tol, φ, env_h, env_r)` + per-species overrides (canopy species raise `max_modules`/`marker_count`/`v_root_max`); the morphology test suite. |
| `src/ecosystem.rs` | `Ecosystem`: the **shared marker field**, `ShadowGrid` (binned, branch-free deposit), carbon-starvation `cull_dead`, seeding, `Climate`/`biome_name`. `step_timed` (per-phase `StepTimings`); **plant-parallel grow** + parallel mesh gather (`trunk_batches`/`foliage_batches`). |
| `src/mesh.rs` | Skeleton → generalized-cylinder mesh; foliage quads; **parallel in-place** per-species-coloured forest mesh (`balanced_ranges`/`carve_mut`/`uninit_vec` → prefix-sum slice fill, no concat). |
| `src/overlay.rs` | 2D clickable Whittaker biome chart. |
| `src/main.rs` | Viewers (`run`, `run_ecosystem`), `run_tree_shot` (`--tree`), `run_shot`, `run_stats` (validation + `loglog_slope`), `run_bench` (`--bench`). |

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
   Each bud (`BudQuery`) has a reveal **ceiling** (rises with age, capped at the
   species height) and a **crown-radius** bound (fills a species-sized cylinder,
   competes in overlaps — no bare limbs racing into open space). A free marker
   goes to the nearest perceiving bud (within r, a forward cone); the bud's
   growth direction `V` is the sum of marker directions. `Q` = space-presence ×
   global-shadow light `g`.
2. **light pass** — `Q` accumulates basipetally → `Q_acc`.
3. **vigor pass** — resource `v = α·Q_base` flows acropetally, split by extended
   Borchert–Honda: `vm = v·λQm/(λQm+(1−λ)Ql)`, `vl = …`.
4. **carbon balance** — `health` (EMA of mean foliage light) is updated; drives
   mortality in the ecosystem.
5. **bud fate** — a bud with resource `v` sprouts `n = ⌊v⌋` metamers of length
   `v/n` (capped at `MAX_SHOOT`/step). Shoot length ∝ vigor. A continuing shoot's
   heading is the **weighted sum of three vectors** (Pałubicki §4.2): the
   *default orientation* (the parent axis heading, weight 1 — the axis stiffness
   that keeps a bole straight), the optimal growth direction `V` (weight `ξ`),
   and tropism (weight `η`, the `Vec3::Y` pull in `sprout`). Heading toward pure
   `V` instead — dropping the default-orientation term — makes axes wander/wiggle
   like worms, since `V` jitters as markers are consumed and neighbours compete.
6. **shedding** (§4.4) — a lateral branch whose mean light is below `shed_ratio`
   is dropped → clean boles under shade (shade-tolerant species keep theirs).
7. **diameters** — pipe model `d = √(Σ d_child²)`, φ at the tips (Eq. 8), so
   **trunk diameter ∝ √(leaf count)**.

**Apical control λ ≈ 0.5** spans excurrent↔decurrent (Pałubicki Fig. 7). Max
height/spread is set by the **marker-cloud envelope** (the principled crown
silhouette), not λ. Species differ only by λ / D (branch angle) / g2 (tropism) /
envelope / niche (climate, shade tolerance, seeding) — plus, for the canopy
species, a bigger module budget so they can grow tall and thick.

**Ecosystem (Sec. 6):** the `ShadowGrid` gives per-bud light `g` (inter-plant
shading). **Carbon-starvation mortality** (`cull_dead`): an established plant
(age > `CARBON_ESTABLISH`) dies when `health` < `CARBON_THRESHOLD` — overtopped,
shaded trees can't pay their upkeep. Shade tolerance floors a species' light, so
tolerant climax species survive shade that kills pioneers → succession. Seeding
+ climate (Eq. 11 Gaussian niche scaling vigor + seeding) → biome composition.

---

## What's done (commit-by-commit, recent first)

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

**25 tests pass.** The `plant.rs` mechanism suite verifies the equations directly
(BH split, `n=⌊v⌋`, basipetal light, pipe model √Σd², shedding, senescence, vigor
conservation, growth bounded). `species.rs` morphology tests are non-degeneracy
sanity (faithfulness over a tidy silhouette).

---

## Quantitative validation (`cargo run -- --stats`)

The headline payoff of getting the mechanisms right — the model agrees with laws
it was never told:

- **Pipe-model allometry** (Eq. 8): trunk diameter vs leaf count → log-log slope
  **≈ 0.51** (predicted 0.50; diameter ∝ √leaves).
- **Self-thinning** (Yoda's −3/2 law): a dense even-aged cohort (the
  `seeding_enabled` flag turns recruitment off) thins while mean biomass rises →
  slope **≈ −1.25…−1.55** (ideal −1.5; the residual is the per-species crown/
  height cap). The shared field is what brought this near −1.5.

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
suite from ~231 s to ~3 s. The single biggest fix was an accidental **O(n²)** —
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
scheduling — so a given seed still yields the same stand and the same mesh on
any machine. The mesh chunk count adapts to cores (capped at `MESH_CHUNKS` /
`GROW_CHUNKS`); `--no-cache`-style determinism is unaffected.

---

## Tuning harness

`--stats` prints an apical-control λ sweep, per-species morphology (height,
trunk_r, slenderness, spread at ~70% lifespan), succession, biome composition,
and the two validation fits. Tune by reading the numbers **and** a `--tree`/
`--shot` PNG. Per-species knobs (`species.rs`): λ (apical control), D (branch
angle), g2 (droop), envelope_height/radius (crown silhouette + size), φ (trunk
thickness scale), max_modules (size budget — canopy species need it high),
v_root_max (resource), climate optima, shade_tolerance, seeding. Global feel
(`PlantParams`/`ecosystem.rs` consts): α, climb_rate, MAX_SHOOT, FIELD_DENSITY,
MAX_FIELD_HEIGHT, OCC_R/PER_R/PER_COS, CARBON_THRESHOLD/ESTABLISH, **`ξ`**
(optimal-direction weight — how hard a shoot bends toward free space vs. holding
its axis; low = straight/stiff, high = wandering/gnarled; default 0.25, global).
`ξ` is a good per-species knob if wanted: low for excurrent (conifer/poplar
leaders ramrod-straight), higher for gnarled decurrent forms (Pałubicki Fig. 12).

---

## Known limitations & gotchas

1. **Performance** — much improved (see the Performance section): a heavy
   tropical frame is now ~21 ms sim + ~13 ms CPU mesh build (was ~150 + ~150),
   and the test suite is **~3 s** (was ~4 min). The remaining interactive cost
   is the **GPU upload** — `Mesh::new` re-uploads all ~2.2M verts every dirty
   frame; `--bench` does NOT measure that. Cutting it is the **LOD / instancing /
   vertex-reduction** future item. Still: **don't run multiple `cargo`
   invocations at once** (they fight the build lock); use `run_in_background`.
   Bench on `--release` and prefer a low-load box (a busy machine inflates all
   phases — watch `loadavg`).
2. **`--shot`/`--tree` "Segmentation fault" ≠ failed render** (exits after writing
   the PNG to skip the Wayland teardown). Always Read the PNG. Check `nvidia-smi`
   if renders OOM.
3. **Occasional reaching limb / lean** — the crown bound mostly stops trees
   racing into open space, but a stand-edge tree can still send one thin limb
   out, or lean toward one-sided light (axis persistence makes a consistent
   sideways pull accumulate rather than cancel; `ξ` and `tropism_up` trade this
   off against straightness). Inspect with `--tree --bare`.
4. **Species presets are adapted, not transcribed** from Tab. 4 (units differ);
   `g2` sign chosen heuristically.
5. The **viewers** still crash on the Wayland teardown at window close (cosmetic;
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
- **Terrain** — elevation lapse rate `T(h)=T(0)+γh` → treelines; soil/blocked map.
- **Richer foliage** — textured/leaf-shaped quads; grass; better materials.
- **More species** — fill out Tab. 4 / Fig. 21; more biome coverage.
- **More validation** — allometry curves (Fig. 16); steeper self-thinning would
  need space-responsive envelopes (the crown/height cap is the −1.5 residual).
- **Window-close teardown** — clean viewer exit (same `process::exit` trick).

---

## Conventions
- Verification here = `cargo test --release` (CPU, ~3 s) + `cargo run -- --stats`
  (CPU) + a `--tree`/`--shot` PNG you actually open; `--bench` for perf. Commit
  freely; small commits preferred.
- See `../CLAUDE.md` for workspace context (mostly Verus-specific; this subproject
  is plain Rust + three-d, no formal verification).
