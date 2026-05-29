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

**Next:** module-orientation optimization + collision-based light (Sec. 5.2.3),
then ecosystem scale — terrain, seeding, global shadow grid, climate/biomes.

## Running

On NixOS the windowing libraries are provided via `shell.nix`:

```sh
./run.sh                 # = nix-shell --run "cargo run --release"
```

Controls: **Space** play/pause · **S** step · **R** reset · **←/→** apical
control λ · **↑/↓** plant growth rate · mouse to orbit/zoom.

Headless tuning sweep (no window): `cargo run -- --stats`.
Tests: `cargo test`.
