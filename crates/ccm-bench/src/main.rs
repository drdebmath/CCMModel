use ccm_core::{NodeId, PortGraph};
use std::env;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let arguments: Vec<String> = env::args().collect();
    if arguments.len() != 5 {
        eprintln!("usage: ccm-bench ALGORITHM NODES AGENTS ITERATIONS");
        std::process::exit(2);
    }
    let algorithm = &arguments[1];
    let nodes = parse(&arguments[2], "nodes");
    let agents = parse(&arguments[3], "agents");
    let iterations = parse(&arguments[4], "iterations");
    if nodes == 0 || agents == 0 || agents > nodes || iterations == 0 {
        eprintln!("nodes, agents, and iterations must be positive; agents must not exceed nodes");
        std::process::exit(2);
    }
    let edges = (0..nodes - 1)
        .map(|node| {
            (
                NodeId(u32::try_from(node).expect("validated node domain")),
                NodeId(u32::try_from(node + 1).expect("validated node domain")),
            )
        })
        .collect::<Vec<_>>();
    let graph = PortGraph::from_undirected_edges(nodes, &edges).expect("valid path");
    let starts = vec![NodeId(0); agents];
    let limit = u64::try_from(agents).unwrap_or(u64::MAX).saturating_mul(40);
    let started = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(match algorithm.as_str() {
            "drop-and-freeze" => {
                let result = ccm_algorithms::simulate(&graph, &starts, limit)
                    .expect("Drop-and-Freeze benchmark must complete");
                result
                    .final_state
                    .agents
                    .iter()
                    .map(|agent| u64::from(agent.node.0) + 1)
                    .sum::<u64>()
            }
            "help-by-scouts" => {
                let result = ccm_help_scouts::run(&graph, &starts, limit)
                    .expect("Help-by-Scouts benchmark must complete");
                result
                    .agents
                    .iter()
                    .map(|agent| u64::from(agent.node.0) + 1)
                    .sum::<u64>()
            }
            _ => {
                eprintln!("unknown algorithm {algorithm:?}");
                std::process::exit(2);
            }
        });
        black_box(checksum);
    }
    println!(
        "{algorithm},{nodes},{agents},{iterations},{},{}",
        started.elapsed().as_nanos(),
        checksum
    );
}

fn parse(value: &str, name: &str) -> usize {
    value.parse().unwrap_or_else(|_| {
        eprintln!("invalid {name}: {value:?}");
        std::process::exit(2);
    })
}
