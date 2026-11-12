//! Validation of `RootedAsync` against the three properties the paper claims:
//!
//! 1. **Dispersion**: every run ends with at most one agent per node, each
//!    agent settled at its own home.
//! 2. **Asynchrony**: correctness does not depend on the schedule. Runs under
//!    random interleavings, with cycles atomic or split into a look and a
//!    later commit, and with any one agent held back, reach the same
//!    placement as synchronous rounds.
//! 3. **Time**: finding the next node takes O(1) epochs, and the whole run
//!    O(k). Checked as its parts:
//!    - every DFS step takes at most `STEP_BOUND` epochs, whatever k or the
//!      degree;
//!    - there are at most 4k steps, since Claim 1 lets each node be entered
//!      at most twice;
//!    - Retrace takes at most `RETRACE_PER_AGENT` epochs per agent.
//!
//! 4. **Memory**: Theorem 2's `O(log(k + Δ))` bits per agent. The most any
//!    agent holds between cycles stays under a constant times `log₂(k + Δ)`,
//!    and that constant does not grow with k.
//!
//! Also checked: Remark 1's bound of three probe batches per node, and
//! Lemma 7's floor of ceil((k-2)/3) scouts.
//!
//! Run with `scripts/validate-rooted-async.sh`, which also runs the ignored
//! large-k test. The bounds are measured, not proved: a finite run can refute
//! O(k), never establish it.

mod common;

use ccm_algorithms::rooted_async::{simulate, AgentState, Position, RootedAsyncResult, Schedule};
use ccm_core::{AgentId, ComplexityMetrics, NodeId, PortAssignment, PortGraph, Termination};
use ccm_experiments::{generate_graph, GraphFamily, GraphSpec};
use ccm_trace::NoTrace;
use common::rooted;

/// Epochs one DFS step may take: probe, decide, vacate, move. Observed at
/// most 30 for k up to 128 and degree up to k - 1.
const STEP_BOUND: u64 = 40;
/// Retrace walks each tree edge at most twice, each move a constant number of
/// epochs. Observed at most 7 per agent.
const RETRACE_PER_AGENT: u64 = 10;
/// Remark 1: three parallel probes cover the k - 2 ports a node may need.
const BATCH_BOUND: u16 = 3;

fn families(k: usize) -> Vec<(&'static str, GraphFamily)> {
    vec![
        ("path", GraphFamily::Path),
        ("star", GraphFamily::Star),
        ("complete", GraphFamily::Complete),
        ("tree3", GraphFamily::Tree { branching: 3 }),
        ("grid8", GraphFamily::Grid { columns: 8 }),
        (
            "random-sparse",
            GraphFamily::RandomConnected { extra_edges: 2 * k },
        ),
        (
            "random-dense",
            GraphFamily::RandomConnected {
                extra_edges: k * k / 4,
            },
        ),
        (
            "bounded-degree",
            GraphFamily::RandomBoundedDegree { max_degree: 4 },
        ),
    ]
}

fn graph(family: &GraphFamily, nodes: usize, seed: u64) -> PortGraph {
    generate_graph(
        &GraphSpec::new(family.clone(), nodes),
        PortAssignment::Random,
        seed,
    )
    .expect("valid family")
}

fn run(g: &PortGraph, agents: usize, schedule: Schedule) -> RootedAsyncResult {
    simulate(
        g,
        &rooted(agents),
        schedule,
        10_000_000,
        ComplexityMetrics::default(),
        NoTrace,
    )
    .unwrap_or_else(|error| panic!("{schedule:?}: {error}"))
    .0
}

fn homes(result: &RootedAsyncResult) -> Vec<NodeId> {
    result
        .agents
        .iter()
        .map(|a| a.home.expect("settled"))
        .collect()
}

/// Property 1, stated directly on the final configuration.
fn assert_dispersed(result: &RootedAsyncResult, nodes: usize, case: &str) {
    assert_eq!(result.termination, Termination::Completed, "{case}");
    let mut agents_at = vec![0_usize; nodes];
    for (index, agent) in result.agents.iter().enumerate() {
        assert_eq!(
            agent.memory.state,
            AgentState::Settled,
            "{case}: agent {index}"
        );
        let Position::At(node) = agent.position else {
            panic!("{case}: agent {index} is still moving");
        };
        assert_eq!(
            agent.home,
            Some(node),
            "{case}: agent {index} is not at home"
        );
        agents_at[node.index()] += 1;
    }
    let crowded = agents_at.iter().position(|&count| count > 1);
    assert_eq!(crowded, None, "{case}: a node holds more than one agent");
}

/// The longest gap between consecutive DFS steps, in epochs.
fn longest_step(result: &RootedAsyncResult) -> u64 {
    let mut previous = 0;
    let mut longest = 0;
    for step in &result.steps {
        longest = longest.max(step.epoch - previous);
        previous = step.epoch;
    }
    longest
}

/// Property 3 for one run, with a line for the report.
fn assert_time_bounds(result: &RootedAsyncResult, k: usize, case: &str) -> String {
    let k64 = k as u64;
    let step = longest_step(result);
    let steps = result.steps.len() as u64;
    let constructed = result.constructed_at.expect("construction ended");
    let retrace = result.epochs - constructed;
    let batches = result.steps.iter().map(|s| s.batches).max().unwrap_or(0);
    let scout_floor = u16::try_from(k.saturating_sub(2).div_ceil(3)).expect("small k");
    let fewest_scouts = result
        .steps
        .iter()
        .filter(|s| s.batches > 0)
        .map(|s| s.scouts)
        .min()
        .unwrap_or(u16::MAX);

    assert!(step <= STEP_BOUND, "{case}: a DFS step took {step} epochs");
    assert!(steps <= 4 * k64, "{case}: {steps} DFS steps for {k} agents");
    assert!(
        retrace <= RETRACE_PER_AGENT * k64,
        "{case}: Retrace took {retrace} epochs"
    );
    assert!(
        batches <= BATCH_BOUND,
        "{case}: {batches} probe batches at one node"
    );
    assert!(
        fewest_scouts >= scout_floor,
        "{case}: only {fewest_scouts} scouts, Lemma 7 promises {scout_floor}"
    );
    format!(
        "{case:<44} {step:>5} {:>7.2} {:>8.2} {batches:>7} {fewest_scouts:>6} {:>8.2}",
        per_agent(steps, k),
        per_agent(retrace, k),
        per_agent(result.epochs, k),
    )
}

/// A count per agent, for the report. The counts are far below 2^52, so the
/// conversion is exact.
#[allow(clippy::cast_precision_loss)]
fn per_agent(count: u64, k: usize) -> f64 {
    count as f64 / k as f64
}

fn report_header() {
    println!(
        "{:<44} {:>5} {:>7} {:>8} {:>7} {:>6} {:>8}",
        "case", "step", "steps/k", "retr/k", "batches", "scouts", "total/k"
    );
}

// ---------------------------------------------------------------------------
// 1. Dispersion
// ---------------------------------------------------------------------------

#[test]
fn every_run_ends_with_at_most_one_agent_per_node() {
    let schedules = [
        Schedule::Synchronous,
        Schedule::Random { seed: 1 },
        Schedule::Random { seed: 2 },
        Schedule::Interleaved { seed: 1 },
        Schedule::Interleaved { seed: 2 },
    ];
    let nodes = 24;
    for (name, family) in families(nodes) {
        for seed in 0..3 {
            let g = graph(&family, nodes, seed);
            // Fewer agents than nodes, about half, and one per node.
            for agents in [3, nodes / 2, nodes] {
                for schedule in schedules {
                    let case = format!("{name} seed {seed}, k={agents}, {schedule:?}");
                    assert_dispersed(&run(&g, agents, schedule), nodes, &case);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Asynchrony
// ---------------------------------------------------------------------------

/// Agents act in an interleaved order, not in lockstep: under a random
/// schedule some agents finish several cycles while others finish one, so
/// there are far more activations than one per agent per epoch.
#[test]
fn random_schedules_really_interleave() {
    let k = 20;
    let g = graph(&GraphFamily::RandomConnected { extra_edges: 30 }, k, 4);
    let sync = run(&g, k, Schedule::Synchronous);
    assert_eq!(
        sync.activations,
        sync.epochs * k as u64,
        "lockstep baseline"
    );
    for seed in 0..5 {
        let result = run(&g, k, Schedule::Random { seed });
        // More than 1.5k activations per epoch, in integers.
        assert!(
            2 * result.activations > 3 * result.epochs * k as u64,
            "seed {seed}: {} activations in {} epochs for {k} agents",
            result.activations,
            result.epochs
        );
    }
}

/// Section 2 lets other agents act between an agent's look and its write.
/// Under `Interleaved` they do, in every run: commits act on views that have
/// gone stale, and no write ever lands on memory changed since its look (that
/// would fail the run as a `WriteConflict`).
#[test]
fn cycles_really_overlap_under_the_interleaved_schedule() {
    let k = 16;
    for (name, family) in families(k) {
        let g = graph(&family, k, 3);
        for seed in 0..4 {
            let result = run(&g, k, Schedule::Interleaved { seed });
            assert!(
                result.stale_commits > 0,
                "{name} seed {seed}: no commit ever acted on a stale view"
            );
        }
    }
    assert_eq!(
        run(
            &graph(&GraphFamily::Path, k, 0),
            k,
            Schedule::Random { seed: 0 }
        )
        .stale_commits,
        0,
        "atomic cycles cannot go stale"
    );
}

/// The schedule never changes where agents end up: decisions depend only on
/// what scouts find, never on who moved first.
#[test]
fn the_schedule_never_changes_the_placement() {
    let nodes = 20;
    for (name, family) in families(nodes) {
        let g = graph(&family, nodes, 2);
        for agents in [7, nodes] {
            let reference = homes(&run(&g, agents, Schedule::Synchronous));
            for seed in 0..8 {
                for schedule in [Schedule::Random { seed }, Schedule::Interleaved { seed }] {
                    let result = run(&g, agents, schedule);
                    assert_eq!(
                        homes(&result),
                        reference,
                        "{name}, k={agents}, {schedule:?}"
                    );
                }
            }
        }
    }
}

/// Holding back any single agent, scout or lead, settled or not, only delays
/// the run. Measured in epochs it does not even do that much: an epoch waits
/// for the slowest agent, so the step bound still holds.
#[test]
fn any_one_slow_agent_only_delays_the_run() {
    let k = 12;
    let g = graph(&GraphFamily::RandomConnected { extra_edges: 18 }, k, 6);
    let reference = homes(&run(&g, k, Schedule::Synchronous));
    for slow in 0..k {
        let schedule = Schedule::Starve {
            seed: 3,
            agent: AgentId(u32::try_from(slow).expect("small k")),
            period: 50,
        };
        let result = run(&g, k, schedule);
        let case = format!("agent {slow} slowed");
        assert_dispersed(&result, k, &case);
        assert_eq!(homes(&result), reference, "{case}");
        assert_time_bounds(&result, k, &case);
    }
}

// ---------------------------------------------------------------------------
// 3. Time
// ---------------------------------------------------------------------------

fn time_bounds_up_to(sizes: &[usize]) {
    report_header();
    for &k in sizes {
        for (name, family) in families(k) {
            for (label, schedule) in [
                ("sync", Schedule::Synchronous),
                ("random", Schedule::Random { seed: 5 }),
                ("interleaved", Schedule::Interleaved { seed: 5 }),
            ] {
                let g = graph(&family, k, 1);
                let result = run(&g, k, schedule);
                let case = format!("{name} k={k} {label}");
                println!("{}", assert_time_bounds(&result, k, &case));
            }
        }
    }
}

#[test]
fn the_next_node_costs_constant_epochs_and_the_run_linear() {
    time_bounds_up_to(&[8, 16, 32, 64]);
}

/// The same check at sizes too slow for every `cargo test`.
#[test]
#[ignore = "slow; run by scripts/validate-rooted-async.sh"]
fn the_time_bounds_hold_up_to_256_agents() {
    time_bounds_up_to(&[128, 256]);
}

/// Degree does not enter: on a complete graph every node has k - 1 ports,
/// yet a step costs no more than on a path.
#[test]
fn a_step_costs_the_same_whatever_the_degree() {
    for k in [8, 32, 96] {
        let path = run(&graph(&GraphFamily::Path, k, 0), k, Schedule::Synchronous);
        let complete = run(
            &graph(&GraphFamily::Complete, k, 0),
            k,
            Schedule::Synchronous,
        );
        assert!(
            longest_step(&complete) <= longest_step(&path),
            "k={k}: a complete-graph step took {} epochs, a path step {}",
            longest_step(&complete),
            longest_step(&path)
        );
    }
}

/// Theorem 2: `O(log(k + Δ))` bits per agent. Returns, per case, the bits and
/// their ratio to `log₂(k + Δ)`.
fn memory_up_to(sizes: &[usize]) -> Vec<(String, u32, f64)> {
    let mut rows = Vec::new();
    for &k in sizes {
        for (name, family) in families(k) {
            let g = graph(&family, k, 1);
            let delta = (0..g.node_count())
                .map(|i| usize::from(g.degree(NodeId(u32::try_from(i).unwrap())).unwrap()))
                .max()
                .unwrap();
            for (label, schedule) in [
                ("sync", Schedule::Synchronous),
                ("random", Schedule::Random { seed: 5 }),
                ("interleaved", Schedule::Interleaved { seed: 5 }),
            ] {
                let result = run(&g, k, schedule);
                #[allow(clippy::cast_precision_loss)]
                let log = ((k + delta) as f64).log2();
                let ratio = f64::from(result.memory_bits) / log;
                let case = format!("{name} k={k} Δ={delta} {label}");
                println!(
                    "{case:<40} {:>5} bits  {ratio:>6.2} × log₂(k+Δ)",
                    result.memory_bits
                );
                rows.push((case, result.memory_bits, ratio));
            }
        }
    }
    rows
}

/// Bits per `log₂(k + Δ)` any run may use. Observed between 24 and 44 for k
/// up to 256 across every family, the ratio falling slightly as k grows.
const MEMORY_PER_LOG: f64 = 48.0;

fn assert_memory_logarithmic(sizes: &[usize]) {
    let rows = memory_up_to(sizes);
    for (case, bits, ratio) in &rows {
        assert!(
            *ratio <= MEMORY_PER_LOG,
            "{case}: {bits} bits is {ratio:.1} × log₂(k+Δ), over {MEMORY_PER_LOG}"
        );
    }
    // Logarithmic, not merely small: the ratio must not climb with k. Compare
    // each family's first and last size.
    let (first, last) = (sizes[0], sizes[sizes.len() - 1]);
    for (name, _) in families(first) {
        let ratio_at = |k: usize| {
            rows.iter()
                .filter(|(case, _, _)| case.starts_with(&format!("{name} k={k} ")))
                .map(|row| row.2)
                .fold(0.0, f64::max)
        };
        assert!(
            ratio_at(last) <= ratio_at(first) * 1.1,
            "{name}: {:.1} × log₂(k+Δ) at k={last} against {:.1} at k={first}",
            ratio_at(last),
            ratio_at(first)
        );
    }
}

#[test]
fn memory_stays_logarithmic_in_k_and_the_degree() {
    assert_memory_logarithmic(&[8, 16, 32, 64]);
}

/// The same check at sizes too slow for every `cargo test`.
#[test]
#[ignore = "slow; run by scripts/validate-rooted-async.sh"]
fn memory_stays_logarithmic_up_to_256_agents() {
    assert_memory_logarithmic(&[8, 64, 128, 256]);
}
