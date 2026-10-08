//! Properties every algorithm must have, checked once for all three.

mod common;

use ccm_core::{ComplexityMetrics, NodeId, PortGraph};
use ccm_trace::{FullTrace, NoTrace, Recorder};
use common::{graph, rooted};

/// Runs one algorithm with the given recorder and returns its final state,
/// rendered for comparison, and its metrics.
fn summary<R: Recorder>(
    algorithm: &str,
    g: &PortGraph,
    starts: &[NodeId],
    recorder: R,
) -> (String, ComplexityMetrics) {
    match algorithm {
        "drop_and_freeze" => {
            let mut metrics = ComplexityMetrics::default();
            let run = ccm_algorithms::drop_and_freeze::simulate_with(
                g,
                starts,
                1_000,
                &mut metrics,
                ccm_algorithms::drop_and_freeze::SharedRecorder::new(recorder),
            )
            .unwrap();
            (
                format!("{:?} {:?}", run.final_state, run.termination),
                metrics,
            )
        }
        "help_by_scouts" => {
            let (result, metrics, _) = ccm_algorithms::help_by_scouts::simulate(
                g,
                starts,
                1_000,
                ComplexityMetrics::default(),
                recorder,
            )
            .unwrap();
            (format!("{result:?}"), metrics)
        }
        "p1tree" => {
            let (result, metrics, _) = ccm_algorithms::p1tree::simulate(
                g,
                starts,
                1_000,
                ComplexityMetrics::default(),
                recorder,
            )
            .unwrap();
            (format!("{result:?}"), metrics)
        }
        other => unreachable!("unknown algorithm {other}"),
    }
}

const ALGORITHMS: [&str; 3] = ["drop_and_freeze", "help_by_scouts", "p1tree"];

fn cases() -> Vec<PortGraph> {
    vec![
        graph(&[(0, 1), (1, 2), (2, 3)], 4),
        graph(&[(0, 1), (1, 2), (2, 3), (3, 0), (0, 2)], 4),
        graph(&[(0, 1), (0, 2), (1, 3), (1, 4)], 5),
    ]
}

#[test]
fn the_same_input_gives_the_same_result() {
    for algorithm in ALGORITHMS {
        for g in cases() {
            let starts = rooted(g.node_count());
            let first = summary(algorithm, &g, &starts, NoTrace);
            let second = summary(algorithm, &g, &starts, NoTrace);
            assert_eq!(first, second, "{algorithm}");
        }
    }
}

#[test]
fn tracing_does_not_change_results_or_metrics() {
    for algorithm in ALGORITHMS {
        for g in cases() {
            let starts = rooted(g.node_count());
            let plain = summary(algorithm, &g, &starts, NoTrace);
            let traced = summary(algorithm, &g, &starts, FullTrace::new());
            assert_eq!(plain, traced, "{algorithm}");
        }
    }
}
