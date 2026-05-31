# HANDOFF — Synthetic Sylvaculture

A Rust reproduction of **Makowski et al. 2019, _Synthetic Silviculture: Multi-scale
Modeling of Plant Ecosystems_** (SIGGRAPH / ACM TOG 38(4)), built on the
self-organizing tree model of **Pałubicki et al. 2009, _Self-organizing Tree
Models for Image Synthesis_**. This file is the orientation map for picking it
back up. Both papers are in the repo:

- `Makowski.etal-2019-Synthetic-Silviculture.pdf` — the ecosystem-scale target.
- `selforg.sig2009.pdf` — Pałubicki 2009, the source for the metamer model,
  extended Borchert–Honda vigor distribution, shadow propagation, and **space
  colonization** (the environment method this project now uses).

> **Model note (read this first):** the plant-growth core was **rewritten** from
> the paper's fixed-prototype "branch module" abstraction to the underlying
> **Pałubicki metamer model**, with **space colonization** as its environment.
> The earlier module model (prototypes, a 9-prototype morphospace, an orientation
> optimizer, `v_min`/`growth_floor`) is **gone** — `src/prototype.rs` is deleted.
> See "Model" below. (Old commits `5a68a1e`…`27dc224` and prior HANDOFF revisions
> describe that retired model.)

---

## Quick start

```sh
cd synthetic-sylvaculture

./run.sh                 # single-plant viewer
./run.sh --eco           # ECOSYSTEM viewer (the main thing)
cargo run -- --stats     # headless tuning/validation readouts (no GPU)
cargo test               # 25 tests (no GPU); SLOW (~5–8 min), see gotchas
./run.sh --tree 0 --steps 120 --shot tree.png         # ONE species, framed solo
./run.sh --shot eco.png --temp 12 --precip 130 --steps 150   # ecosystem frame
```

**NixOS:** the GUI must run through `./run.sh` (enters `shell.nix`). Plain `cargo
run` fails — winit dlopens the Wayland/X11 libs and they're not on the default
loader path. GL comes from `/run/opengl-driver/lib`.

**Reading the PDFs here:** no `pdftoppm`, so the Read tool can't render pages.
Extract text with `nix-shell -p poppler-utils --run "pdftotext file.pdf out.txt"`.

### Viewer controls
- **Single-plant** (`./run.sh`): Space play/pause · S step · R reset ·
  ←/→ apical control λ · ↑/↓ growth rate · N cycle species · F foliage.
- **Ecosystem** (`./run.sh --eco`): Space · S · R reseed · F foliage · **click the
  biome triangle (top-left)** or ←/→ temperature, ↑/↓ precipitation to set climate.

### Self-verifying renders (the GUI can't be watched in this environment)
`--tree`/`--shot` render off-screen to a PNG you then open/inspect. **A
"Segmentation fault" from these is NOT a failed render** — they call
`std::process::exit(0)` after saving to dodge a Wayland-teardown segfault; the
PNG is already written. Always judge by the PNG. If renders genuinely OOM-crash,
check `nvidia-smi` (a full GPU looks exactly like a geometry bug).

---

## File map

| File | What |
|---|---|
| `src/plant.rs` | The core: `Plant`, `Internode` (metamer), `PlantParams`; the growth cycle (space colonization → light → Borchert–Honda vigor → bud fate/sprout → shed → pipe-model diameters); `skeleton()`/`leaves()`/`shape()`/`module_centres()`. |
| `src/species.rs` | 7 plant-type presets (`preset(λ, D, gp, v_root_max, g2, s_tol, φ, env_h, env_r)` + climate/seeding traits) and the morphology test suite. |
| `src/ecosystem.rs` | `Ecosystem`: many plants; `ShadowGrid` (global inter-plant shading → per-bud `g`); seeding, senescence/cull, `Climate` + `biome_name`. |
| `src/mesh.rs` | Skeleton → generalized-cylinder `CpuMesh`; foliage leaf-quad fans; per-species-coloured forest mesh builders. API-driven (`Segment`, `(pos,dir)`), untouched by the rewrite. |
| `src/overlay.rs` | 2D clickable Whittaker biome chart (screen↔climate mapping). |
| `src/main.rs` | Viewers (`run` single-plant, `run_ecosystem`), `run_tree_shot` (`--tree`), `run_shot` (`--shot`), `run_stats`. |
| ~~`src/prototype.rs`~~ | **Deleted** — the morphospace prototypes are obsolete under procedural metamer growth. |

---

## Model (Pałubicki metamer + space colonization)

A plant is a tree of **metamers** (`Internode`: an internode + an axillary
lateral bud; an axis tip also carries a terminal bud). `Plant::new(params,
origin)`. Each `step` (Pałubicki Fig. 3):

1. **environment — space colonization (§4.1).** Each plant has a dome-shaped
   cloud of free-space **markers** (`generate_markers`, sized by
   `envelope_height`×`envelope_radius`). Each step: consume markers within ρ
   (`occupancy_radius`) of any bud; associate each remaining marker to the
   nearest bud that perceives it (within `perception_radius` and a forward cone
   `perception_cos`); a bud's `Q` = (has markers?) × global-shadow light `g`, and
   its growth direction `V` = normalized sum of directions to those markers.
   A **reachable ceiling** rises with age (`climb_rate`) so the dome is revealed
   bottom-up → **gradual growth** (not an instant pop). A bud with no free space
   gets `Q=0` and stops — this **bounds the leader** and fills the crown, and
   removes the need for any separate orientation/collision optimizer.
2. **light pass** — `Q` accumulates basipetally into `q_acc`.
3. **vigor pass** — resource `v = α·Q_base` flows acropetally, split at each
   branch by extended Borchert–Honda: `vm = v·λ·Qm/(λQm+(1−λ)Ql)`, `vl = …`.
4. **bud fate** — a bud with resource `v` sprouts `n = ⌊v⌋` metamers of length
   `v/n` (**shoot length ∝ vigor**), capped at `MAX_SHOOT`/step; shoots steer
   toward `V` (open space).
5. **shedding** (§4.4) — a branch with a low light/size ratio is dropped
   (`shed_ratio`; **off by default** — this is the lever for clean boles).
6. **diameters** — pipe model `d = √(Σ d_child²)`, φ at the tips (Eq. 8).

**Apical control λ ≈ 0.5** spans excurrent↔decurrent (Pałubicki Fig. 7; λ>0.5
leader-biased). **Maximum height/spread is set by the marker-cloud envelope**,
not λ — the envelope IS the principled crown-silhouette control (the paper's
crown shaping): poplar a tall narrow column, conifer a tall narrow spire, oak a
short broad crown, shrub small. Species differ only by `λ / D (branch angle) /
g2 (tropism) / envelope / niche (climate, shade tolerance, seeding)` — **no
per-species silhouette hacks** (the paper's diversity is emergent, not
hand-authored).

**Ecosystem scale (Sec. 6, unchanged by the rewrite):** the `ShadowGrid` casts
downward pyramidal penumbrae; each plant reads per-bud `g` from it for
inter-plant competition (self-thinning, succession). Seeding/flowering + senescence
(`p_max`) + cull drive succession & gap dynamics. Climatic adaptation (Eq. 11)
scales `v_root_max` + seeding by a per-species Gaussian niche → biome composition.

---

## What's done (commit-by-commit, recent first)

- `c2d9e0d` **Growth-rate smoothing** — the rising-`climb_rate` ceiling reveals the
  envelope bottom-up, so a tree grows gradually over tens of steps (and reaches
  its full envelope height with proper taper) instead of consuming the whole
  cloud in ~12 steps.
- `f06896b` **Space colonization** (§4.1) — free-space marker competition as the
  environment. Fixes the runaway leader (the metamer model's main failure):
  bounded, full-crowned, envelope-differentiated trees.
- `b984f04` **Metamer-model rewrite** — replaced the fixed-prototype module growth
  with the faithful Pałubicki metamer model (BH split, `n=⌊v⌋` rule, pipe model).
  Deleted `prototype.rs`. (Faithful but, on its own, whippy — hence space
  colonization above.)
- `547aed5` **Correctness test suite** — one test per paper mechanism.
- earlier (`802dfa6`…`27dc224`): the retired **module model** (M1–M5: prototypes,
  orientation optimizer, foliage, ecosystem, shadow grid, climate/biomes,
  `--shot`/`--tree` harness). Most of that infrastructure (ecosystem, mesh,
  overlay, shot modes, climate) carried forward; only the plant-growth core changed.

**25 tests pass.** The `plant.rs` mechanism suite verifies the paper's *equations*
directly (BH split λ:(1−λ), the `n=⌊v⌋` metamer rule, basipetal light, pipe model
√Σd², shedding, senescence, vigor conservation). The `species.rs` morphology tests
are **non-degeneracy sanity** (not aesthetic bands) — faithfulness was chosen over
a tidy silhouette, so don't re-pin "pretty" thresholds.

---

## Tuning harness

`cargo run -- --stats` prints an apical-control λ sweep, per-species morphology
(height / trunk_r / slenderness / spread / apex_lean, at ~70% of each species'
lifespan), shadowing biomass cut, succession, and biome composition. Tune by
reading the numbers **and** a `--tree`/`--shot` PNG together.

Per-species knobs (`species.rs::preset`): **λ** (apical control / leader vs lateral
balance), **D** (lateral branch angle), **g2** (lateral droop), **envelope_height /
envelope_radius** (the crown silhouette — the main shape control), **φ** (trunk
thickness), **v_root_max** (resource budget; climate scales it), plus climate optima
+ seeding. Global growth feel: `alpha`, `climb_rate`, `MAX_SHOOT`, `internode_len`.

---

## Known limitations & gotchas

1. **Trees are a touch shrubby — no clean bare bole.** Branches start near the
   ground because `shed_ratio = 0` (shedding off). Turning shedding on is exactly
   how the paper grows "tall boles" (Pałubicki §4.4): shaded lower branches drop.
   This is the most visible remaining realism lever and the natural next task.
2. **The test suite is slow (~5–8 min)** — the metamer model has many more nodes
   than the old module model, so the ecosystem tests do much more work per step.
   **Do not run multiple `cargo` invocations at once** — they fight the build lock
   and a run can balloon to 15+ min. Run one at a time (use `run_in_background`
   and wait for the completion notification).
3. **`--shot`/`--tree` "Segmentation fault" is not a failed render** — it exits via
   `process::exit(0)` after writing the PNG to skip the Wayland teardown crash.
   Always Read the PNG. If renders OOM-crash, check `nvidia-smi`.
4. **Species presets are adapted, not transcribed**, from Tab. 4 (the paper's units
   differ from this model's scales). `g2` tropism sign was chosen heuristically.
5. The single-plant and ecosystem **viewers** still crash on the same Wayland
   teardown at window close (cosmetic — happens after you're done; only the
   scripted `--shot`/`--tree` paths were fixed).
6. **Forest canopy** reflects the climate's dominant species — a poplar-dominated
   stand reads as narrow columns; a broad-species climate reads more canopy-like.

---

## What remains / future work

Roughly in priority order:

- **Shedding for clean boles** — enable/tune `shed_ratio` so shaded lower branches
  drop, giving clear trunks (Pałubicki §4.4 "tall bole"). Biggest visible win.
- **Per-species form polish** — tune envelopes / D / g2 (e.g. a cone-shaped
  envelope for a sharper conifer spire; flatter acacia crown).
- **Self-shading within a single tree** — `--tree` shots use only the marker field
  (no neighbour shade); in the forest the global grid already self-shades. A
  per-plant shadow pass would make isolated trees self-thin too.
- **Forest LOD / instancing** — to scale past a few hundred plants.
- **Terrain** — elevation lapse rate `T(h)=T(0)+γh` → treelines; soil/blocked map.
- **Validation plots** — the 3/2 self-thinning power law (Fig. 14) and allometry
  (Fig. 16) as `--stats` outputs / tests.
- **Window-close teardown** — make the viewers exit cleanly (same `process::exit`).

---

## Conventions
- Verification here = `cargo test` (CPU) + `cargo run -- --stats` (CPU) + a
  `--tree`/`--shot` PNG you actually open. Commit freely; small commits preferred.
- See `../CLAUDE.md` for broader workspace context (mostly Verus-specific; this
  subproject is plain Rust + three-d, no formal verification).
