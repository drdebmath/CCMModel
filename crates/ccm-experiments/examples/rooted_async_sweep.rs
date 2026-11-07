//! Runs `RootedAsync` over every small configuration and reports any run in
//! which a decision made from the local view disagreed with the truth.
//!
//! ```sh
//! cargo run --release -p ccm-experiments --example rooted_async_sweep          # synchronous
//! cargo run --release -p ccm-experiments --example rooted_async_sweep -- 11    # random, seed 11
//! cargo run --release -p ccm-experiments --example rooted_async_sweep -- i11   # interleaved, seed 11
//! ```

use ccm_algorithms::rooted_async::{simulate, Schedule};
use ccm_core::{ComplexityMetrics, NodeId, PortAssignment};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};
use ccm_trace::NoTrace;
use std::collections::BTreeMap;

fn main() {
    let schedule = std::env::args()
        .nth(1)
        .map_or(Schedule::Synchronous, |arg| match arg.strip_prefix('i') {
            Some(seed) => Schedule::Interleaved {
                seed: seed.parse().expect("the seed is a number"),
            },
            None => Schedule::Random {
                seed: arg.parse().expect("the seed is a number"),
            },
        });
    let families = [
        GraphFamily::Path,
        GraphFamily::Cycle,
        GraphFamily::Star,
        GraphFamily::Complete,
        GraphFamily::Tree { branching: 3 },
        GraphFamily::Grid { columns: 4 },
        GraphFamily::RandomConnected { extra_edges: 10 },
        GraphFamily::RandomBoundedDegree { max_degree: 4 },
    ];
    let assignments = [
        PortAssignment::Canonical,
        PortAssignment::Random,
        PortAssignment::Adversarial,
    ];
    let mut runs = 0_u64;
    let (mut stale, mut stale_runs) = (0_u64, 0_u64);
    let mut failures: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for nodes in [5_usize, 6, 7, 8, 10, 12, 16] {
        for family in &families {
            for assignment in assignments {
                for seed in 0..6 {
                    let spec = GraphSpec::new(family.clone(), nodes);
                    let Ok(graph) = generate_graph(&spec, assignment, seed) else {
                        continue;
                    };
                    for agents in 2..=nodes {
                        runs += 1;
                        let starts = vec![NodeId(0); agents];
                        let outcome = simulate(
                            &graph,
                            &starts,
                            schedule,
                            1_000_000,
                            ComplexityMetrics::default(),
                            NoTrace,
                        );
                        if let Ok((result, _, _)) = &outcome {
                            stale += result.stale_commits;
                            stale_runs += u64::from(result.stale_commits > 0);
                        }
                        if let Err(error) = outcome {
                            let case = format!(
                                "n={nodes} {} {assignment:?} seed={seed} k={agents}",
                                family.label()
                            );
                            failures.entry(format!("{error:?}")).or_default().push(case);
                        }
                    }
                }
            }
        }
    }
    println!(
        "{schedule:?}: {runs} runs, {} failed",
        failures.values().map(Vec::len).sum::<usize>()
    );
    if matches!(schedule, Schedule::Interleaved { .. }) {
        println!("  {stale} commits acted on a stale view, in {stale_runs} of the runs");
    }
    for (error, cases) in &failures {
        println!("  {error}: {} runs, e.g. {}", cases.len(), cases[0]);
    }
}
