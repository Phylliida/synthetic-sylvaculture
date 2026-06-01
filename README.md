# synthetic-sylvaculture

A Rust reproduction of **Makowski et al. 2019, _Synthetic Silviculture:
Multi-scale Modeling of Plant Ecosystems_** (SIGGRAPH / ACM TOG 38(4)), built on
the self-organizing tree model of **Pałubicki et al. 2009, _Self-organizing Tree
Models for Image Synthesis_** — with a native 3D viewer to watch a forest grow.

## What it is

A **self-organizing forest**. Individual trees grow by the Pałubicki **metamer
model**; the whole stand competes for one **shared free-space marker field**
(space colonization); trees are bounded and shaped by that competition and by
light, self-prune clean boles, and **die of carbon starvation** when overtopped.
So succession (pioneer → climax), self-thinning, and a layered canopy all
*emerge* — they are tuned by a small set of per-species parameters, not scripted.

This is the headline of the paper, reproduced: plant diversity emerges from the
selection pressures of climate and competition, not from per-species hand-tuning.

## How it works (one growth cycle, per plant)

1. **Space colonization** (Pałubicki §4.1) — buds compete for free-space markers;
   a bud grows toward the markers it perceives and stops once its space is taken.
2. **Light** flows basipetally to the root; **vigor** (resource) flows back up,
   split at each branch by the extended **Borchert–Honda** rule (apical control λ
   spans excurrent ↔ decurrent forms).
3. A bud with resource `v` sprouts `⌊v⌋` metamers (shoot length ∝ vigor).
4. **Shedding** drops shaded branches → clean boles; **pipe-model** diameters
   (`d = √Σd_child²`) → trunk thickness ∝ √(leaf count).
5. **Ecosystem**: a global shadow grid gives inter-plant shading; **carbon
   starvation** kills overtopped trees; climate (a Gaussian niche) + seeding
   drive **biome composition**.

The model agrees with laws it was never told (`--stats`): **pipe-model allometry**
(diameter ∝ √leaves, log-log slope ≈ 0.5) and **Yoda's −3/2 self-thinning law**
(slope ≈ −1.5 for a dense cohort).

See **`HANDOFF.md`** for the full orientation map (architecture, parameters,
validation, performance, and gotchas). The original fixed-prototype "branch
module" model that earlier versions used has been replaced; this is the
self-organizing metamer model throughout.

## Running

On NixOS the windowing libraries are provided via `shell.nix`:

```sh
./run.sh --eco           # ECOSYSTEM viewer (the main thing)
./run.sh                 # single-plant viewer (N cycles species)
cargo run -- --stats     # headless readouts incl. quantitative validation
cargo run --release -- --bench   # headless performance benchmark (sim + mesh)
cargo test --release     # 25 tests (CPU, ~3 s)
./run.sh --tree 6 --steps 200 --shot t.png                 # one species, framed
./run.sh --shot e.png --temp 26 --precip 320 --steps 170   # an ecosystem frame
```

**Ecosystem controls:** Space play/pause · S step · R reseed · F foliage · ←/→
temperature · ↑/↓ precipitation, or click the Whittaker biome chart (top-left).
Mouse orbits/zooms.

## Performance

The simulation and the per-frame mesh build are parallelized (`std::thread`, no
extra deps) and were tuned across nine measured rounds: a worst-case tropical
stand (~170 plants, ~80k modules) runs the sim at **~21 ms/step** (was ~149) and
rebuilds its **~2.2M-vertex** mesh in **~13 ms/frame** (was ~148); the test suite
is **~3 s** (was ~4 min). The optimizations are bit-identical to the sequential
versions (a given seed yields the same stand and mesh on any machine). The
remaining interactive cost is the GPU vertex upload — see `HANDOFF.md` (LOD /
instancing). Run `--bench` for the per-phase breakdown.
