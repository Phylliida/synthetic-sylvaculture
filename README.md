# synthetic-sylvaculture

A Rust reproduction of **Makowski et al. 2019, _Synthetic Silviculture:
Multi-scale Modeling of Plant Ecosystems_** (SIGGRAPH / ACM TOG 38(4)).

The paper models plant ecosystems at three scales — *module* (a small skeletal
branching graph), *plant* (a self-organizing tree of modules), and *ecosystem*
(many plants under climate, shadowing and seeding). This repo rebuilds that
model incrementally, with a native 3D viewer to watch plants grow.

## Status

**Milestone 1 — single-plant growth (done).**
- Branch-module prototypes as skeletal graphs with a morphospace (λ, D) — Sec. 5.1
- Extended Borchert–Honda light/vigor distribution with apical control λ — Eq. 2
- Module development: growth-rate sigmoid (Eq. 5), physiological-age integration
  (Eq. 6), per-segment acropetal timing (Eq. 7), Pipe-Model diameters (Eq. 8),
  length growth (Eq. 9), tropism offset (Eq. 10)
- Generalized-cylinder surface mesh
- Native interactive viewer (three-d): orbit camera, play/step/reset, live λ tweak

Reproduced qualitative results: high apical control → tall trunk-dominated
(*excurrent*) forms; low apical control → short bushy (*decurrent*) forms; the
Pipe Model yields a trunk thicker than its twigs. (See `cargo test`.)

**Milestone 2 — orientation optimization + light competition (done).**
- Spherical module bounding volumes; collision-based light `Q = exp(−f_collisions)`
  (Eq. 1) feeding the vigor pass — shaded/crowded branches grow less, so
  self-thinning emerges
- Module orientation stored relative to parent; a per-step gradient-descent
  relaxation minimizes `f_distribution = ω1·f_collisions + ω2·f_tropism`
  (Eqs. 3–4, 12–13, App. A.1), so the crown self-organizes to avoid collisions
- Validated against Fig. 15a: intersection-volume ratio drops from ~5.4% (naive)
  to ~0.3% (optimized), under the paper's 5% target. Toggle live with O / L.

**Milestone 3 — visual polish (done).**
- Foliage: leaf-quad fans on twigs, deterministic (flicker-free), green; F toggles
- Nine-prototype morphospace via a parametric generator (branch angle from λ,
  monopodial↔sympodial from D, 3D azimuthal spread); a vigorous parent gets
  monopodial modules, a weak tip sympodial — intra-tree structural variety
- Five species presets (conifer / poplar / birch / oak / shrub), each with its
  own form + leaf/bark colour; cycle with N. Adapted from Tab. 4's character
  (the table's units differ from this model's scales)
- Warm key + cool fill directional lights + ambient, lighter sky

**Milestone 4 — ecosystem (done).**
- Many mixed-species plants on flat ground, one combined per-species-coloured
  mesh; `./run.sh --eco`
- Global shadow-propagation grid (Pałubicki / Sec. 6.2): downward pyramidal
  penumbrae in a voxel grid; `Q_eff = lerp(s_tol, 1, Q·Q_G)` with per-species
  shade tolerance
- Root vigor scales with captured light (`v_base = α·Q_base`, capped) → shaded
  plants are suppressed. Validated: global shadowing cuts total biomass ~36%
  via competition (`cargo run -- --stats`)
- Flowering + seeding (Sec. 6.3): plants past flowering age scatter seeds of
  their species nearby; senescence (p_max) + culling open gaps → **succession
  and gap dynamics** (pioneer shrubs boom, then yield to longer-lived species)
- Climatic adaptation (Sec. 6.4, Eq. 11): temperature/precipitation scale each
  species' growth + seeding via a Gaussian niche → **biome composition** (cold →
  shrub tundra, temperate → poplar forest, warm/wet → oak). ←/→/↑/↓ set the
  climate; a Whittaker-style label names the biome
- Validated in `--stats`: shadowing −65% biomass, succession over 360 steps,
  biome dominants across three climates. 10 tests pass.

## Running

On NixOS the windowing libraries are provided via `shell.nix`:

```sh
./run.sh                 # = nix-shell --run "cargo run --release"
```

Controls: **Space** play/pause · **S** step · **R** reset · **←/→** apical
control λ · **↑/↓** plant growth rate · mouse to orbit/zoom.

Headless tuning sweep (no window): `cargo run -- --stats`.
Tests: `cargo test`.
