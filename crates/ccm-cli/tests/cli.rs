//! End-to-end checks of the `ccm` binary: `RootedDisp` through `run` and
//! `sweep`, with fixed graph, port and seed inputs.

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
    assert!(String::from_utf8_lossy(&output.stderr).contains("NonRootedStart"));
}
