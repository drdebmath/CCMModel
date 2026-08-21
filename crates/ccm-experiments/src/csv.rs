use crate::runner::{algorithm_label, termination_label, RunResult};

/// Stable tabular schema: exactly one row per independent simulation trial.
pub const CSV_HEADER: &str = "algorithm,algorithm_version,git_commit,graph_model,port_assignment,placement,nodes,edges,agents,seed,starts,completed,termination,rounds,macro_rounds,probe_rounds,movement_rounds,agent_moves,edge_traversals,port_probes,probe_out_traversals,probe_back_traversals,settlements,backtracks,group_moves,maximum_group_size,maximum_simultaneously_unsettled,maximum_tree_depth,peak_owned_nodes,scout_operations,vacates,chase_operations,follow_operations,retraces,peak_logical_memory_words\n";

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
    use ccm_core::{Algorithm, ComplexityMetrics, NodeId, Termination};

    #[test]
    fn csv_has_header_and_one_row_per_result() {
        let result = RunResult {
            algorithm: Algorithm::DropAndFreeze,
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
        };
        let csv = results_to_csv(&[result.clone(), result]);
        assert_eq!(csv.lines().count(), 3);
        assert!(csv.starts_with(CSV_HEADER.trim_end()));
        assert!(csv.contains(
            "drop_and_freeze,0.1.0,test-commit,path,canonical,single:0,3,2,3,4,0;0;0,true,completed"
        ));
    }
}
