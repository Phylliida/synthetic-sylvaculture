# HANDOFF — Synthetic Sylvaculture

A Rust reproduction of **Makowski et al. 2019, _Synthetic Silviculture: Multi-scale
Modeling of Plant Ecosystems_** (SIGGRAPH / ACM TOG 38(4)). Built from scratch in one
session. This file is the orientation map for picking it back up.

The two papers it's based on are in the repo:
- `Makowski.etal-2019-Synthetic-Silviculture.pdf` — the target paper.
- `selforg.sig2009.pdf` — Pałubicki et al. 2009, "Self-organizing Tree Models", the
  source for the extended Borchert–Honda vigor model and the shadow-propagation grid.

---

## Quick start

```sh
cd synthetic-sylvaculture

./run.sh                 # single-plant viewer  (= nix-shell --run "cargo run --release")
./run.sh --eco           # ECOSYSTEM viewer (the main thing)
cargo run -- --stats     # headless tuning/validation readouts (no GPU needed)
cargo test               # 20 tests (no GPU needed)
./run.sh --shot out.png --temp 12 --precip 130 --steps 180   # render one frame to PNG
```

**NixOS note:** the GUI must run through `./run.sh` (which enters `shell.nix`). Plain
`cargo run` fails — winit dlopen's the Wayland/X11 libs at runtime and they're not on the
default loader path. GL comes from `/run/opengl-driver/lib`.

### Viewer controls
- **Single-plant** (`./run.sh`): Space play/pause · S step · R reset · ←/→ apical control λ ·
  ↑/↓ growth rate · N cycle species · O orientation-opt · L collision-light · F foliage.
- **Ecosystem** (`./run.sh --eco`): Space · S · R reseed · F foliage · **click the biome
  triangle (top-left)** or ←/→ temperature, ↑/↓ precipitation to set the climate/biome.

---

## File map

| File | What |
|---|---|
| `src/prototype.rs` | Branch-module prototypes (skeletal graphs); 9 on a (λ, D) morphospace grid. Apical terminal is always vertical. |
| `src/plant.rs` | The core: `Plant`, `Module`, `PlantParams`; growth step (light → vigor → develop → shed), Borchert–Honda, Pipe-Model diameters, orientation optimizer, `place()`/`skeleton()`/`shape()`/`module_spheres()`. |
| `src/mesh.rs` | Skeleton → generalized-cylinder `CpuMesh`; foliage leaf-quad fans; per-species-coloured forest mesh builders. |
| `src/species.rs` | 7 plant-type presets (`preset(...)` structural params + climate/seeding traits) and the morphology test suite. |
| `src/ecosystem.rs` | `Ecosystem`: many plants, `ShadowGrid` (global shadowing), seeding, senescence/cull, `Climate` + `biome_name`. |
| `src/overlay.rs` | 2D clickable Whittaker biome chart (screen↔climate mapping). |
| `src/main.rs` | Viewers (`run` single-plant, `run_ecosystem`), `run_shot` (PNG), `run_stats`. |

---

## Paper → code mapping (the model)

- **Branch modules** (Sec 5.1): skeletal graphs, root + terminals, `terminals[0]` = apical.
- **Borchert–Honda vigor** (Eq 2): basipetal light accumulate, acropetal redistribute with
  apical control λ. Root vigor scales with captured light (`v_base = α·Q_base`, capped at
  `v_root_max`) so shaded plants are suppressed. → `plant.rs::vigor_pass`.
- **Module development** (Eqs 5–10): growth-rate sigmoid + floor, physiological-age
  integration, acropetal per-segment timing (Eq 7), Pipe-Model diameter (Eq 8), length
  (Eq 9), tropism offset (Eq 10). → `plant.rs::develop` / `place`.
- **Orientation optimization** (Sec 5.2.3, App A.1): per-step gradient descent minimizing
  `f_distribution = ω1·f_collisions + ω2·f_tropism`; freezes settled modules (anti-flicker),
  damped. Collision light `Q = exp(−scale·f_collisions)`. → `plant.rs::optimize_orientations`.
- **Morphospace** (Sec 5.2.2): Voronoi-nearest prototype to (λ, D′), D′ = parent vigor ratio.
- **Global shadowing** (Sec 6.2 / Pałubicki): voxel grid, downward pyramidal penumbra
  `Δs = a·b^−q`, `Q_eff = lerp(s_tol, 1, Q·Q_G)`. → `ecosystem.rs::ShadowGrid`.
- **Seeding/flowering** (Sec 6.3): Gaussian seed scatter past flowering age; senescence
  (`p_max`) + cull → succession & gap dynamics.
- **Climatic adaptation** (Sec 6.4, Eq 11): per-species temperature/precipitation Gaussian
  niche scales growth + seeding → the biomes.

---

## What's done (commit-by-commit)

All five milestones of the reproduction, plus tuning and fixes:

1. **Scaffold** — three-d viewer, NixOS `shell.nix`.
2. **M1 single-plant growth** — Borchert–Honda + dev Eqs 5–10 + Pipe Model + mesh.
3. **M2 orientation optimization + light competition** (Sec 5.2.3); intersection ratio
   5.4%→0.3% (Fig 15a). + **crown-flicker fix** (freeze settled modules + damp).
4. **M3 polish** — foliage, 9-prototype morphospace, species presets, key/fill lighting.
5. **M4 ecosystem** — E1 many plants + combined mesh; E2 shadow grid + light-limited vigor
   (−65% biomass under competition); E3 seeding/senescence/climate → biomes & succession;
   clickable biome-chart overlay; `--shot` screenshot harness.
6. **M5 tree shape** — per-species φ (size-varied trunks), straight leaders (fix banana/loop),
   `growth_floor` for tall-and-leafy crowns + `max_modules` cap; shape-metric tuning harness.
7. **Fixes** — `--shot` exit segfault (Wayland teardown after PNG written); 0 warnings.

Reproduced qualitative results from the paper: excurrent↔decurrent from λ; Pipe-Model
trunk thickening; intersection-volume ratio < 5% (Fig 15a); self-thinning under shade;
succession (pioneers boom then yield); biome composition shifting with climate.

**20 tests pass** (`plant` 7, `species` 6, `ecosystem` 4, `overlay` 3). `--stats` prints
apical-control sweep, orientation ratio, forest arc, succession, biome composition, tree
morphology, and forest-mesh-size diagnostics.

---

## Tuning harness

`Plant::shape()` → `(height, crown_radius, apex_offset)`. Derived: **slenderness** = h/diam,
**spread** = crown/h, **apex_lean** = apex_offset/h (arc measure). `cargo run -- --stats`
prints these per species (measured at ~70% of each species' lifespan). Tune by reading the
numbers *and* a `--shot` PNG together. Tests in `species.rs`/`ecosystem.rs` pin the expected
look (excurrent tower over broad, every species has a crown, no banana trunks, trunk thickens
with size, plausible slenderness, forest canopy stays upright).

Key per-species knobs (`species.rs::preset`): λ (apical control → height/narrowness),
D (determinacy → lateral count), v_root_max (budget/size), φ (trunk thickness),
`growth_floor` (crown fullness), `max_modules` (geometry cap), climate optima + seeding.

---

## Known limitations & gotchas

1. **Tall emergents are spindly.** Tall + narrow ⇒ sparse foliage by the Pipe Model
   (the leading shoot is thin). The understory carries the lushness, so the *forest* reads
   well, but a lone tall tree looks thin. Genuinely lush tall trees would need the paper's
   per-bud **metamer model** (a bud emits ⌊v⌋ metamers, so branch length ∝ vigor) — a real
   rewrite of module development, not a param tweak. This is the biggest open quality item.
2. **`--shot` rendering gotchas** (both bit me hard this session — both now understood):
   - It used to segfault *on exit* (winit/Wayland teardown) **after** writing a valid PNG —
     fixed with `std::process::exit(0)` after save. A "Segmentation fault" from `--shot` no
     longer means a failed render; **always Read the PNG to judge**.
   - If the **GPU VRAM fills** (e.g. another process holding it — an RTX 3090 was at 21/24 GB
     once), every render OOM-segfaults in the GL alloc path and looks exactly like a geometry
     bug. Check `nvidia-smi` if renders crash.
3. **Combined forest mesh** is one big `CpuMesh` rebuilt each sim step — fine at current
   scale (caps: `max_plants` 170, `max_modules` 150/plant) but it has no LOD/instancing, so
   it won't scale to the paper's 500K plants. Mesh-size diagnostic in `--stats`.
4. **Species presets are adapted, not transcribed**, from Tab. 4 (the paper's units differ
   from this model's scales). `g2` tropism sign was chosen heuristically.
5. The single-plant and ecosystem viewers also crash on the same Wayland teardown at window
   close (cosmetic — happens after you're done; only `--shot` was fixed since it's scripted).

---

## What remains / future work

Roughly in priority order:

- **Lusher tall trees** — implement the metamer model (branch length ∝ vigor) so tall trees
  aren't spindly. This is the main visual gap. (See limitation #1.)
- **Forest LOD / instancing** — to scale past a few hundred plants (billboards/imposters at
  distance, or GPU instancing of module prototypes as the paper does), and to fix the
  one-giant-mesh bottleneck.
- **Terrain** — the paper has elevation (temperature lapse rate `T(h)=T(0)+γh` → treelines),
  a soil/blocked map, and renders on a heightmap. Currently flat ground.
- **Richer foliage** — textured leaf quads / per-species leaf shapes; grass; better materials.
- **More species** — fill out the Tab. 4 / Fig. 21 library; more biome coverage.
- **Validation plots** — reproduce the 3/2 self-thinning power law (Fig 14) and allometry
  curves (Fig 16) as `--stats` outputs / tests.
- **Interactive niceties** — on-screen text/HUD (needs `three-d-text-builder`), plant
  selection/removal, save/load of a stand.
- **Window-close teardown** — make the viewers exit cleanly too (same `process::exit` trick).

---

## Conventions
- `./check.sh`-style verification here = `cargo test` (CPU) + `cargo run -- --stats` (CPU) +
  a `--shot` PNG you actually open. Commit freely; small commits preferred.
- See `../CLAUDE.md` for the broader workspace context (note: most of it is Verus-specific;
  this subproject is plain Rust + three-d, no formal verification).
