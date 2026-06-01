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
cargo test               # 25 tests (no GPU); SLOW (~4 min) — see gotchas
./run.sh --tree 6 --steps 200 --shot t.png            # ONE species, framed solo
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
| `src/plant.rs` | The core. `Plant` (metamers + buds), the growth cycle, the `colonize`/`Occ`/`BudQuery`/`PointGrid` space-colonization core, self-shadow, shedding, pipe-model diameters, `health` (carbon balance), and geometry queries (`skeleton`/`leaves`/`shape`/`biomass`/`module_centres`/`active_buds`). |
| `src/species.rs` | 7 plant-type presets `preset(λ, D, gp, v_root_max, g2, s_tol, φ, env_h, env_r)` + per-species overrides (canopy species raise `max_modules`/`marker_count`/`v_root_max`); the morphology test suite. |
| `src/ecosystem.rs` | `Ecosystem`: the **shared marker field**, `ShadowGrid` (inter-plant light), carbon-starvation `cull_dead`, seeding, `Climate`/`biome_name`. |
| `src/mesh.rs` | Skeleton → generalized-cylinder mesh; foliage quads; per-species-coloured forest mesh. API-driven; untouched by the model work. |
| `src/overlay.rs` | 2D clickable Whittaker biome chart. |
| `src/main.rs` | Viewers (`run`, `run_ecosystem`), `run_tree_shot` (`--tree`), `run_shot`, `run_stats` (incl. validation + a `loglog_slope` fitter). |

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
   `v/n` (capped at `MAX_SHOOT`/step), steered toward `V`. Shoot length ∝ vigor.
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
  slope **≈ −1.25…−1.37** (ideal −1.5; the residual is the per-species crown/
  height cap). The shared field is what brought this near −1.5.

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
MAX_FIELD_HEIGHT, OCC_R/PER_R/PER_COS, CARBON_THRESHOLD/ESTABLISH.

---

## Known limitations & gotchas

1. **Performance is the main ceiling.** Bigger canopy trees mean more metamers →
   heavier sim and mesh. A forest renders ~20–27 s / 170 steps (≈120–160 ms/step
   — still animates in the viewer, just statelier). The **test suite is ~4 min**.
   **Do not run multiple `cargo` invocations at once** — they fight the build lock
   and a run can balloon; use `run_in_background` and wait. Smooth play at full
   jungle scale wants the **LOD/instancing** on the future-work list.
2. **`--shot`/`--tree` "Segmentation fault" ≠ failed render** (exits after writing
   the PNG to skip the Wayland teardown). Always Read the PNG. Check `nvidia-smi`
   if renders OOM.
3. **Occasional reaching limb** — the crown bound mostly stops trees racing into
   open space, but a stand-edge tree can still send one thin limb out.
4. **Species presets are adapted, not transcribed** from Tab. 4 (units differ);
   `g2` sign chosen heuristically.
5. The **viewers** still crash on the Wayland teardown at window close (cosmetic;
   only the scripted `--shot`/`--tree` paths were fixed).
6. `README.md` is **stale** — it still describes the old module-model milestones.

---

## What remains / future work (priority order)

- **Forest LOD / instancing** — the big one now: needed for smooth interactive
  play at canopy scale and to scale past a few hundred plants.
- **Terrain** — elevation lapse rate `T(h)=T(0)+γh` → treelines; soil/blocked map.
- **Richer foliage** — textured/leaf-shaped quads; grass; better materials.
- **More species** — fill out Tab. 4 / Fig. 21; more biome coverage.
- **More validation** — allometry curves (Fig. 16); steeper self-thinning would
  need space-responsive envelopes (the crown/height cap is the −1.5 residual).
- **Window-close teardown** — clean viewer exit (same `process::exit` trick).
- **Refresh `README.md`** to the current model.

---

## Conventions
- Verification here = `cargo test` (CPU) + `cargo run -- --stats` (CPU) + a
  `--tree`/`--shot` PNG you actually open. Commit freely; small commits preferred.
- See `../CLAUDE.md` for workspace context (mostly Verus-specific; this subproject
  is plain Rust + three-d, no formal verification).
