# Documentation

Start here.

## How the algorithms work

- **[The interactive flowcharts](https://drdebmath.github.io/CCMModel/docs.html)** —
  the same three charts as a page you can pan and zoom, where clicking any box
  opens the source of the function it stands for. Served alongside the
  simulator; also reachable from either app page's header.
- **[algorithm-flowcharts.md](algorithm-flowcharts.md)** — one flowchart per
  algorithm, with the source of every function each chart names. The best place
  to start if you want to know what actually runs.
- [p1tree.md](p1tree.md) — P1Tree dispersion, implemented from Pattanayak,
  Kshemkalyani, Kumar, Molla and Sharma, *Optimal Dispersion Under Asynchrony*.
  What is implemented, what is not, and the two places where the pseudocode
  needs reading alongside the proofs.
- [behavior-spec.md](behavior-spec.md) — Drop-and-Freeze and Help-by-Scouts,
  as they behaved in the Python implementation the Rust ports preserve.
  **Archival**: it cites files this repository no longer contains.

## How to read the numbers

- [complexity-metrics.md](complexity-metrics.md) — what each counter in the CSV
  means and what it does not. Read before quoting any of them. **Archival** in
  the same sense as above.
- [benchmarks.md](benchmarks.md) — the Python-versus-Rust runtime measurement
  that motivated the migration. **Archival**: the implementation it compared
  against has been removed, so it is no longer reproducible from this tree.

Wall-clock time is never a complexity result. The counters are.

## How the repository fits together

- [architecture.md](architecture.md) — crate boundaries and what each one owns.
- [simulation-model.md](simulation-model.md) — the model the core implements:
  ports, agents, rounds, termination.
- [migration.md](migration.md) — how this became a Rust-only repository, and
  which compatibility decisions were deliberate.

## A note on the diagrams

There are two versions of the flowcharts, and they are generated from the same
crates by different means.

`algorithm-flowcharts.md` uses [mermaid](https://mermaid.js.org) blocks. GitHub
renders them inline; the copy GitHub Pages serves does not, and shows the
diagram source as a code block instead. Read that one here on GitHub.

`docs.html` draws its own SVG against the application's theme tokens, so it
follows the light/dark toggle and needs no renderer at all. That is the one to
open in a browser.
