//! Epochs `RootedAsync` takes as the number of agents grows, under the
//! synchronous and a random schedule. The figures in
//! `docs/2507.01298-optimal-dispersion-under-asynchrony/gap-and-trace-note.md`.
//!
//! ```sh
//! cargo run --release -p ccm-experiments --example rooted_async_scaling
//! ```

use ccm_algorithms::rooted_async::{simulate, Schedule};
use ccm_core::{ComplexityMetrics, NodeId, PortAssignment};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};
use ccm_trace::NoTrace;

fn main() {
    println!("graph,k,schedule,epochs,vacates");
    for k in [16_usize, 32, 64, 128, 256] {
        let families = [
            ("path", GraphFamily::Path),
            ("tree3", GraphFamily::Tree { branching: 3 }),
            ("grid8", GraphFamily::Grid { columns: 8 }),
            (
                "random+2k",
                GraphFamily::RandomConnected { extra_edges: 2 * k },
            ),
            ("complete", GraphFamily::Complete),
        ];
        for (name, family) in families {
            let graph = generate_graph(&GraphSpec::new(family, k), PortAssignment::Random, 3)
                .expect("valid family");
            for (label, schedule) in [
                ("sync", Schedule::Synchronous),
                ("random", Schedule::Random { seed: 1 }),
            ] {
                let (result, metrics, _) = simulate(
                    &graph,
                    &vec![NodeId(0); k],
                    schedule,
                    10_000_000,
                    ComplexityMetrics::default(),
                    NoTrace,
                )
                .expect("the run completes");
                println!("{name},{k},{label},{},{}", result.epochs, metrics.vacates);
            }
        }
    }
}
