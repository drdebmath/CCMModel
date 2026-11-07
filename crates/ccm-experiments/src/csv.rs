use crate::runner::{
    algorithm_kind, algorithm_label, scheduler_label, termination_label, time_unit, RunResult,
};

/// Stable tabular schema: exactly one row per independent simulation trial.
pub const CSV_HEADER: &str = "algorithm,algorithm_version,git_commit,graph_model,port_assignment,placement,nodes,edges,agents,seed,starts,completed,termination,rounds,macro_rounds,probe_rounds,movement_rounds,agent_moves,edge_traversals,port_probes,probe_out_traversals,probe_back_traversals,settlements,backtracks,group_moves,maximum_group_size,maximum_simultaneously_unsettled,maximum_tree_depth,peak_owned_nodes,scout_operations,vacates,chase_operations,follow_operations,retraces,peak_logical_memory_words,algorithm_kind,scheduler,time_unit\n";

/// Serializes rows without adding a filesystem or CSV-library dependency.
/// Values containing commas, quotes, or newlines are RFC-4180 escaped.
#[must_use]
pub fn results_to_csv(results: &[RunResult]) -> String {
    let mut output = String::with_capacity(CSV_HEADER.len() + results.len() * 256);
    output.push_str(CSV_HEADER);
    for result in results {
        let starts = result
            .starts
            .iter()
            .map(|node| node.0.to_string())
            .collect::<Vec<_>>()
            .join(";");
        let values = [
            algorithm_label(result.algorithm).to_owned(),
            result.algorithm_version.to_owned(),
            result.git_commit.to_owned(),
            result.graph_family.clone(),
            result.port_assignment.to_owned(),
            result.placement.clone(),
            result.nodes.to_string(),
            result.edges.to_string(),
            result.agents.to_string(),
            result.seed.to_string(),
            starts,
            result.completed().to_string(),
            termination_label(&result.termination).to_owned(),
            result.metrics.rounds.to_string(),
            result.metrics.macro_rounds.to_string(),
            result.metrics.probe_rounds.to_string(),
            result.metrics.movement_rounds.to_string(),
            result.metrics.agent_moves.to_string(),
            result.metrics.edge_traversals.to_string(),
            result.metrics.port_probes.to_string(),
            result.metrics.probe_out_traversals.to_string(),
            result.metrics.probe_back_traversals.to_string(),
            result.metrics.settlements.to_string(),
            result.metrics.backtracks.to_string(),
            result.metrics.group_moves.to_string(),
            result.metrics.maximum_group_size.to_string(),
            result.metrics.maximum_simultaneously_unsettled.to_string(),
            result.metrics.maximum_tree_depth.to_string(),
            result.metrics.peak_owned_nodes.to_string(),
            result.metrics.scout_operations.to_string(),
            result.metrics.vacates.to_string(),
            result.metrics.chase_operations.to_string(),
            result.metrics.follow_operations.to_string(),
            result.metrics.retraces.to_string(),
            result.metrics.peak_logical_memory_words.to_string(),
            algorithm_kind(result.algorithm).to_owned(),
            scheduler_label(result.scheduler),
            time_unit(result.scheduler).to_owned(),
        ];
        for (index, value) in values.iter().enumerate() {
            if index != 0 {
                output.push(',');
            }
            write_csv_value(&mut output, value);
        }
        output.push('\n');
    }
    output
}

fn write_csv_value(output: &mut String, value: &str) {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        output.push('"');
        output.push_str(&value.replace('"', "\"\""));
        output.push('"');
    } else {
        output.push_str(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccm_core::{Algorithm, ComplexityMetrics, NodeId, SchedulerModel, Termination};

    fn result(algorithm: Algorithm) -> RunResult {
        RunResult {
            algorithm,
            algorithm_version: "0.1.0",
            git_commit: "test-commit",
            graph_family: "path".to_owned(),
            port_assignment: "canonical",
            placement: "single:0".to_owned(),
            nodes: 3,
            edges: 2,
            agents: 3,
            seed: 4,
            starts: vec![NodeId(0), NodeId(0), NodeId(0)],
            termination: Termination::Completed,
            metrics: ComplexityMetrics::default(),
            scheduler: SchedulerModel::Synchronous,
        }
    }

    #[test]
    fn csv_has_header_and_one_row_per_result() {
        let row = result(Algorithm::DropAndFreeze);
        let csv = results_to_csv(&[row.clone(), row]);
        assert_eq!(csv.lines().count(), 3);
        assert!(csv.starts_with(CSV_HEADER.trim_end()));
        assert!(csv.contains(
            "drop_and_freeze,0.1.0,test-commit,path,canonical,single:0,3,2,3,4,0;0;0,true,completed"
        ));
    }

    fn column(name: &str) -> usize {
        CSV_HEADER
            .trim_end()
            .split(',')
            .position(|column| column == name)
            .unwrap_or_else(|| panic!("no {name} column"))
    }

    fn cell(row: &RunResult, name: &str) -> String {
        let csv = results_to_csv(std::slice::from_ref(row));
        let cells: Vec<String> = csv
            .lines()
            .nth(1)
            .unwrap()
            .split(',')
            .map(str::to_owned)
            .collect();
        assert_eq!(cells.len(), CSV_HEADER.trim_end().split(',').count());
        cells[column(name)].clone()
    }

    /// New columns are appended, so every older column keeps its position.
    #[test]
    fn columns_are_only_ever_appended() {
        assert_eq!(column("peak_logical_memory_words"), 34);
        assert_eq!(column("algorithm_kind"), 35);
        assert_eq!(column("scheduler"), 36);
        assert_eq!(column("time_unit"), 37);
    }

    #[test]
    fn algorithm_kind_marks_legacy_baselines() {
        for (algorithm, kind) in [
            (Algorithm::DropAndFreeze, "legacy_baseline"),
            (Algorithm::HelpByScouts, "legacy_baseline"),
            (Algorithm::P1Tree, "paper"),
            (Algorithm::RootedAsync, "paper"),
            (Algorithm::RootedDisp, "paper"),
        ] {
            assert_eq!(
                cell(&result(algorithm), "algorithm_kind"),
                kind,
                "{algorithm:?}"
            );
        }
    }

    /// Rounds and epochs are not comparable, so each row says which it holds.
    #[test]
    fn every_row_names_its_scheduler_and_time_unit() {
        let mut row = result(Algorithm::RootedAsync);
        for (scheduler, label, unit) in [
            (SchedulerModel::Synchronous, "synchronous", "round"),
            (SchedulerModel::Asynchronous, "asynchronous", "epoch"),
            (SchedulerModel::Interleaved, "interleaved", "epoch"),
            (
                SchedulerModel::Starved {
                    agent: 2,
                    period: 9,
                },
                "starved:2:9",
                "epoch",
            ),
        ] {
            row.scheduler = scheduler;
            assert_eq!(cell(&row, "scheduler"), label);
            assert_eq!(cell(&row, "time_unit"), unit);
        }
    }
}
