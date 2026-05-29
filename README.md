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

**Next:** ecosystem scale — terrain, seeding, global shadow grid, climate/biomes;
plus filling the 9-prototype morphospace and foliage.

## Running

On NixOS the windowing libraries are provided via `shell.nix`:

```sh
./run.sh                 # = nix-shell --run "cargo run --release"
```

Controls: **Space** play/pause · **S** step · **R** reset · **←/→** apical
control λ · **↑/↓** plant growth rate · mouse to orbit/zoom.

Headless tuning sweep (no window): `cargo run -- --stats`.
Tests: `cargo test`.
