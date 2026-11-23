//! End-to-end checks of the `ccm` binary: `RootedDisp`, `GeneralDisp` and
//! leader election and rendezvous through `run`, `sweep` and `transitions`,
//! with fixed graph, port and seed inputs, and the placements each algorithm
//! refuses.

use std::process::{Command, Output};

fn ccm(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ccm"))
        .args(args)
        .output()
        .expect("the ccm binary runs")
}

/// The CSV rows of a successful run, each as `(column name, value)` pairs.
fn rows(output: &Output) -> Vec<Vec<(String, String)>> {
    assert!(
        output.status.success(),
        "ccm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout.clone()).expect("CSV is UTF-8");
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().expect("a header row").split(',').collect();
    lines
        .map(|line| {
            header
                .iter()
                .zip(line.split(','))
                .map(|(name, value)| ((*name).to_owned(), value.to_owned()))
                .collect()
        })
        .collect()
}

fn field<'a>(row: &'a [(String, String)], name: &str) -> &'a str {
    row.iter()
        .find(|(column, _)| column == name)
        .map_or_else(|| panic!("no column {name}"), |(_, value)| value.as_str())
}

const STAR: [&str; 13] = [
    "run",
    "--algorithm",
    "rooted-disp",
    "--graph",
    "star",
    "--nodes",
    "9",
    "--agents",
    "9",
    "--ports",
    "canonical",
    "--placement",
    "single:0",
];

#[test]
fn run_reproduces_the_star_hand_trace() {
    let mut args = STAR.to_vec();
    args.extend(["--seed", "1"]);
    let first = ccm(&args);
    let table = rows(&first);
    assert_eq!(table.len(), 1);
    let row = &table[0];
    assert_eq!(field(row, "algorithm"), "rooted_disp");
    assert_eq!(field(row, "algorithm_kind"), "paper");
    assert_eq!(field(row, "termination"), "completed");
    assert_eq!(field(row, "rounds"), "73");
    assert_eq!(field(row, "probe_rounds"), "44");
    assert_eq!(field(row, "agent_moves"), "264");
    assert_eq!(field(row, "port_probes"), "65");
    assert_eq!(field(row, "backtracks"), "7");
    // Fixed inputs give the same bytes on every run.
    assert_eq!(ccm(&args).stdout, first.stdout);
}

#[test]
fn every_alias_names_the_same_algorithm() {
    for alias in [
        "rooted-disp",
        "rooted_disp",
        "rooteddisp",
        "heo-dfs",
        "rooted-heo-dfs",
    ] {
        let mut args = STAR.to_vec();
        args[2] = alias;
        assert_eq!(
            field(&rows(&ccm(&args))[0], "algorithm"),
            "rooted_disp",
            "{alias}"
        );
    }
}

#[test]
fn sweep_emits_one_row_per_nodes_agents_and_seed() {
    let output = ccm(&[
        "sweep",
        "--algorithm",
        "rooted-disp",
        "--graph",
        "random-connected:4",
        "--nodes",
        "10,20",
        "--agents",
        "4,10",
        "--seeds",
        "1,2,3",
        "--ports",
        "random",
        "--placement",
        "single:0",
        "--workers",
        "3",
    ]);
    let table = rows(&output);
    assert_eq!(table.len(), 2 * 2 * 3);
    for row in &table {
        assert_eq!(field(row, "algorithm"), "rooted_disp");
        assert_eq!(field(row, "termination"), "completed");
    }
    let order: Vec<(&str, &str, &str)> = table
        .iter()
        .map(|row| {
            (
                field(row, "nodes"),
                field(row, "agents"),
                field(row, "seed"),
            )
        })
        .collect();
    assert_eq!(order[0], ("10", "4", "1"));
    assert_eq!(order[1], ("10", "4", "2"));
    assert_eq!(order[3], ("10", "10", "1"));
    assert_eq!(order[6], ("20", "4", "1"));
}

#[test]
fn a_scattered_placement_is_refused() {
    let output = ccm(&[
        "run",
        "--algorithm",
        "rooted-disp",
        "--graph",
        "path",
        "--nodes",
        "8",
        "--agents",
        "4",
        "--placement",
        "uniform:4",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("RequiresRootedStart"));
}

#[test]
fn every_rooted_only_algorithm_refuses_a_scattered_placement() {
    for algorithm in ["help-by-scouts", "p1tree", "rooted-disp"] {
        let output = ccm(&[
            "run",
            "--algorithm",
            algorithm,
            "--graph",
            "path",
            "--nodes",
            "8",
            "--agents",
            "4",
            "--placement",
            "clustered:0,7",
        ]);
        assert!(!output.status.success(), "{algorithm}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("RequiresRootedStart"),
            "{algorithm}: {error}"
        );
        assert!(error.contains("clustered:0;7"), "{algorithm}: {error}");
    }
}

/// The two-groups case of `general_disp.rs`'s tests, from the command line.
const TWO_GROUPS: [&str; 11] = [
    "--algorithm",
    "general-disp",
    "--graph",
    "path",
    "--nodes",
    "6",
    "--agents",
    "6",
    "--placement",
    "explicit:0,0,0,0,3,3",
    "--seed",
];

#[test]
fn general_disp_runs_from_a_scattered_start() {
    let mut args = vec!["run"];
    args.extend(TWO_GROUPS);
    args.push("1");
    let first = ccm(&args);
    let table = rows(&first);
    assert_eq!(table.len(), 1);
    let row = &table[0];
    assert_eq!(field(row, "algorithm"), "general_disp");
    assert_eq!(field(row, "algorithm_kind"), "paper");
    assert_eq!(field(row, "termination"), "completed");
    assert_eq!(field(row, "starts"), "0;0;0;0;3;3");
    // Twelve-step units: 131 steps is 11 units, the last one cut short when
    // the configuration became legitimate.
    assert_eq!(field(row, "rounds"), "131");
    assert_eq!(field(row, "macro_rounds"), "11");
    assert_eq!(field(row, "settlements"), "5");
    assert_eq!(ccm(&args).stdout, first.stdout);
}

#[test]
fn every_general_disp_alias_names_the_same_algorithm() {
    for alias in ["general-disp", "general_disp", "generaldisp"] {
        let mut args = vec!["run"];
        args.extend(TWO_GROUPS);
        args.push("1");
        args[2] = alias;
        assert_eq!(
            field(&rows(&ccm(&args))[0], "algorithm"),
            "general_disp",
            "{alias}"
        );
    }
}

#[test]
fn a_general_disp_sweep_over_scattered_starts_completes() {
    for placement in ["uniform:3", "uniform:10", "clustered:0,9"] {
        let output = ccm(&[
            "sweep",
            "--algorithm",
            "general-disp",
            "--graph",
            "random-connected:8",
            "--nodes",
            "10,30",
            "--agents",
            "5,10",
            "--seeds",
            "1,2",
            "--ports",
            "adversarial",
            "--placement",
            placement,
            "--workers",
            "2",
        ]);
        let table = rows(&output);
        assert_eq!(table.len(), 2 * 2 * 2, "{placement}");
        for row in &table {
            assert_eq!(field(row, "termination"), "completed", "{placement}");
        }
    }
}

#[test]
fn transitions_lists_the_merge_of_two_groups() {
    let mut args = vec!["transitions"];
    args.extend(TWO_GROUPS);
    args.push("1");
    let output = ccm(&args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("step,slot,event,agent,node,detail"));
    let rows: Vec<&str> = lines.collect();
    // Slot 1 of step 0: the weaker agents on each start node become zombies.
    assert!(rows.contains(&"0,1,became_zombie,0,0,stronger=1:3"));
    assert!(rows.contains(&"0,1,became_zombie,4,3,stronger=1:5"));
    // Group A rises to level 2 at once; its settlers carry that label.
    assert!(rows.contains(&"1,2,level_up,3,0,level=2 zombie=1"));
    assert!(rows.contains(&"1,2,settled,0,0,group=2:3"));
    // B's leader, waiting on node 2, loses to A; A then absorbs B's settler.
    assert!(rows.contains(&"48,1,became_zombie,5,2,stronger=2:3"));
    assert!(rows.contains(&"73,2,incorporated,4,3,from=1:5 to=2:3"));
    // Every row's slot agrees with its step.
    for row in &rows {
        let mut fields = row.split(',');
        let step: u64 = fields.next().unwrap().parse().unwrap();
        let slot: u64 = fields.next().unwrap().parse().unwrap();
        assert_eq!(slot, step % 12 + 1, "{row}");
    }
}

/// The problem page's example: a path with IDs 3, 2, 1 from one end.
const PAGE_EXAMPLE: [&str; 15] = [
    "run",
    "--algorithm",
    "leader-election",
    "--graph",
    "path",
    "--nodes",
    "3",
    "--agents",
    "3",
    "--ports",
    "canonical",
    "--placement",
    "explicit:2,1,0",
    "--seed",
    "1",
];

#[test]
fn leader_election_reproduces_the_page_example() {
    let first = ccm(&PAGE_EXAMPLE);
    let table = rows(&first);
    assert_eq!(table.len(), 1);
    let row = &table[0];
    assert_eq!(field(row, "algorithm"), "leader_election");
    assert_eq!(field(row, "algorithm_kind"), "paper");
    assert_eq!(field(row, "termination"), "completed");
    assert_eq!(field(row, "placement"), "explicit:2;1;0");
    assert_eq!(field(row, "rounds"), "16");
    assert_eq!(field(row, "probe_rounds"), "16");
    assert_eq!(field(row, "macro_rounds"), "2");
    assert_eq!(field(row, "port_probes"), "10");
    assert_eq!(field(row, "agent_moves"), "20");
    assert_eq!(field(row, "settlements"), "0");
    assert_eq!(field(row, "peak_logical_memory_words"), "27");
    assert_eq!(ccm(&PAGE_EXAMPLE).stdout, first.stdout);
}

#[test]
fn every_leader_election_alias_names_the_same_algorithm() {
    for alias in ["leader-election", "leader_election", "leaderelection"] {
        let mut args = PAGE_EXAMPLE.to_vec();
        args[2] = alias;
        assert_eq!(
            field(&rows(&ccm(&args))[0], "algorithm"),
            "leader_election",
            "{alias}"
        );
    }
}

#[test]
fn a_leader_election_sweep_runs_dispersed_and_rooted() {
    for (placement, settlements) in [("dispersed", "0"), ("single:0", "12")] {
        let output = ccm(&[
            "sweep",
            "--algorithm",
            "leader-election",
            "--graph",
            "random-connected:6",
            "--nodes",
            "12",
            "--agents",
            "12",
            "--seeds",
            "1,2,3",
            "--ports",
            "random",
            "--placement",
            placement,
            "--workers",
            "2",
        ]);
        let table = rows(&output);
        assert_eq!(table.len(), 3, "{placement}");
        for row in &table {
            assert_eq!(field(row, "termination"), "completed", "{placement}");
            assert_eq!(field(row, "settlements"), settlements, "{placement}");
            // n − 1 = 11 simulated message-passing rounds.
            assert_eq!(field(row, "macro_rounds"), "11", "{placement}");
        }
    }
}

#[test]
fn leader_election_refuses_starts_it_is_not_defined_for() {
    for (agents, placement) in [("4", "dispersed"), ("8", "clustered:0,7")] {
        let output = ccm(&[
            "run",
            "--algorithm",
            "leader-election",
            "--graph",
            "path",
            "--nodes",
            "8",
            "--agents",
            agents,
            "--placement",
            placement,
        ]);
        assert!(!output.status.success(), "{placement}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("RequiresDispersedOrRootedStart"),
            "{placement}: {error}"
        );
    }
}

#[test]
fn transitions_is_only_for_general_disp() {
    let output = ccm(&[
        "transitions",
        "--algorithm",
        "rooted-disp",
        "--graph",
        "path",
        "--nodes",
        "4",
        "--agents",
        "4",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("AlgorithmUnavailable"));
}

/// Rendezvous through `run`: the labels reach the CSV, and the meeting comes
/// within the exact limit 2(n - 1)(1 + |shorter string|): here 10 * (1 + 4).
#[test]
fn rendezvous_runs_with_its_labels_and_meets_within_the_limit() {
    let output = ccm(&[
        "run",
        "--algorithm",
        "rendezvous",
        "--graph",
        "path",
        "--nodes",
        "6",
        "--agents",
        "2",
        "--placement",
        "explicit:0,5",
        "--labels",
        "1,3",
    ]);
    let rows = rows(&output);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(field(row, "algorithm"), "rendezvous");
    assert_eq!(field(row, "algorithm_kind"), "paper");
    assert_eq!(field(row, "completed"), "true");
    assert_eq!(field(row, "labels"), "1;3");
    assert!(field(row, "rounds").parse::<u64>().unwrap() <= 50);
    assert!(field(row, "memory_bits_per_agent").parse::<u64>().unwrap() > 0);
}

/// A sweep over random trees: every seed draws another tree, and all meet.
#[test]
fn rendezvous_sweeps_random_trees() {
    let output = ccm(&[
        "sweep",
        "--algorithm",
        "rendezvous",
        "--graph",
        "random-tree",
        "--nodes",
        "20",
        "--agents",
        "2",
        "--placement",
        "explicit:3,17",
        "--ports",
        "random",
        "--seed",
        "1,2,3,4,5",
    ]);
    let rows = rows(&output);
    assert_eq!(rows.len(), 5);
    for row in &rows {
        assert_eq!(field(row, "graph_model"), "random-tree");
        assert_eq!(field(row, "edges"), "19");
        assert_eq!(field(row, "completed"), "true");
        // Default labels 1 and 2: strings of length 4 and 6.
        assert_eq!(field(row, "labels"), "1;2");
        assert!(field(row, "rounds").parse::<u64>().unwrap() <= 38 * 5);
    }
}

#[test]
fn rendezvous_refuses_a_graph_that_is_not_a_tree() {
    let output = ccm(&[
        "run",
        "--algorithm",
        "rendezvous",
        "--graph",
        "cycle",
        "--nodes",
        "6",
        "--agents",
        "2",
        "--placement",
        "explicit:0,3",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("trees only"));
}
