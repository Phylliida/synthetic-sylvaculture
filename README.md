# synthetic-sylvaculture

A Rust reproduction of **Makowski et al. 2019, _Synthetic Silviculture:
Multi-scale Modeling of Plant Ecosystems_** (SIGGRAPH / ACM TOG 38(4)), built on
the self-organizing tree model of **Pałubicki et al. 2009, _Self-organizing Tree
Models for Image Synthesis_** — with a native 3D viewer to watch a forest grow.

## What it is

An **evolving self-organizing forest**. Individual trees grow by the Pałubicki
**metamer model** + **space colonization** (the whole stand competes for one
shared free-space marker field). On top of that, the ecosystem **evolves**: there
is no fixed species list — each plant has a heritable **genome**, founders are
uniform-random, and seeds inherit the parent genome with mutation. Climate is
*not* in the genome and there is no hardcoded niche; it acts only as physics (two
factors — warmth and water — stressing different traits). So each biome's plant
community **specializes by natural selection**, and diversity within it is held
open by negative frequency-dependence (Janzen–Connell). Succession,
self-thinning, a layered canopy, *and* climate-shaped morphology all emerge —
nothing is scripted.

This is the headline of the paper, taken one step further: plant diversity
emerges from the selection pressures of climate and competition — here, literally
by evolving genomes, not by hand-tuned species.

## How it works

*Per plant, each growth cycle:* buds compete for free-space markers (**space
colonization**); light flows basipetally and **vigor** flows back up, split by the
extended **Borchert–Honda** rule (apical control λ spans excurrent ↔ decurrent);
a bud sprouts `⌊v⌋` metamers (length ∝ vigor); **determinacy** sets monopodial vs
**sympodial** (relay) branching; shaded branches are **shed**; **pipe-model**
diameters give trunk ∝ √(leaf count).

*Across the ecosystem:* a shadow grid gives inter-plant shading; a 2D **climate**
(warmth → growth & crown shape, water → crown size) sets who can pay their carbon
upkeep; plants **die** of carbon starvation or heritable old age (→ gap churn);
they reproduce only when **lit**, scatter a constant **seed rain**, and
**Janzen–Connell** crowding keeps rare strategies alive. Genome → colour, so a
specializing biome is *seen* to converge.

Validation it was never told (`--stats`): **pipe-model allometry** (diameter ∝
√leaves, log-log slope ≈ 0.51 ✓). **Yoda's self-thinning** has the right sign but
is ≈ −1.25 (ideal −1.5) — survivors expand into freed space as the stand thins
(space-responsive crowns); the residual is the establishment crash (see
`HANDOFF.md`).

See **`HANDOFF.md`** for the full orientation map (architecture, the genome,
parameters, validation, performance, and gotchas).

## Running

On NixOS the windowing libraries are provided via `shell.nix`:

```sh
./run.sh --eco           # ECOSYSTEM viewer (the main thing — the evolving forest)
./run.sh                 # single-plant viewer (cycle the archetype species)
cargo run -- --stats     # headless: evolution trace, 2D specialization, validation
cargo run --release -- --bench   # headless performance benchmark (sim + mesh)
cargo test --release     # 27 tests (CPU)
./run.sh --tree 6 --steps 200 --shot t.png                 # one archetype, framed
./run.sh --shot e.png --temp 26 --precip 320 --steps 170   # an ecosystem frame
```

**Ecosystem controls:** Space play/pause · S step · R reseed · F foliage · ←/→
temperature · ↑/↓ precipitation · −/= shrink/grow the plot · PageDown/PageUp
lower/raise the growth ceiling · or click the labelled Whittaker biome chart
(top-left). Mouse orbits/zooms.

## Performance

The simulation and the per-frame mesh build are parallelized (`std::thread`, no
extra deps) and were tuned across nine measured rounds: a worst-case tropical
stand (~170 plants) runs the sim at **~8–21 ms/step** and rebuilds its mesh in
**~7–13 ms/frame** (each was ~150). The test suite is ~18 s (most of it the
multi-seed evolution test; the mechanism tests are a few seconds). The
parallelism optimizations are bit-identical to the sequential
versions (a given seed yields the same stand and mesh on any machine). The
remaining interactive cost is the GPU vertex upload — see `HANDOFF.md` (LOD /
instancing). Run `--bench` for the per-phase breakdown.
