use ccm_core::{Algorithm, NodeId, PortAssignment};
use ccm_experiments::{
    execute_one, prepare_run, results_to_csv, run_sweep_parallel, BuiltinRunner, GraphFamily,
    GraphSpec, PlacementSpec, RunRequest, SweepSpec,
};
use std::env;
use std::fmt;

#[derive(Debug)]
enum CliError {
    Help,
    Usage(String),
    Parse(String),
    Preparation(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Help => Ok(()),
            Self::Usage(message) | Self::Parse(message) | Self::Preparation(message) => {
                f.write_str(message)
            }
        }
    }
}

fn main() {
    match run() {
        Ok(()) | Err(CliError::Help) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

fn run() -> Result<(), CliError> {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".to_owned());
    if command == "help" || command == "--help" || command == "-h" {
        print_help();
        return Ok(());
    }
    let values: Vec<String> = args.collect();
    let options = Options::parse(&values)?;
    match command.as_str() {
        "validate" => validate(&options),
        "run" => run_plan(&options),
        "sweep" => sweep_plan(&options),
        _ => Err(CliError::Usage(format!("unknown command {command:?}"))),
    }
}

#[derive(Clone, Debug)]
struct Options {
    algorithm: Algorithm,
    graph: GraphFamily,
    nodes: Vec<usize>,
    agents: Vec<usize>,
    seed: Vec<u64>,
    ports: PortAssignment,
    placement: PlacementSpec,
    round_limit: Option<u64>,
    workers: usize,
}

impl Options {
    fn parse(values: &[String]) -> Result<Self, CliError> {
        let mut options = Self {
            algorithm: Algorithm::DropAndFreeze,
            graph: GraphFamily::Path,
            nodes: vec![8],
            agents: vec![4],
            seed: vec![42],
            ports: PortAssignment::Canonical,
            placement: PlacementSpec::SingleNode { node: NodeId(0) },
            round_limit: None,
            workers: 1,
        };
        let mut index = 0;
        while index < values.len() {
            let flag = values[index].as_str();
            let value = |index: &mut usize| -> Result<&str, CliError> {
                *index += 1;
                values
                    .get(*index)
                    .map(String::as_str)
                    .ok_or_else(|| CliError::Usage(format!("{flag} requires a value")))
            };
            match flag {
                "--algorithm" => options.algorithm = parse_algorithm(value(&mut index)?)?,
                "--graph" => options.graph = parse_graph(value(&mut index)?)?,
                "--nodes" => options.nodes = parse_list(value(&mut index)?)?,
                "--agents" => options.agents = parse_list(value(&mut index)?)?,
                "--seed" | "--seeds" => options.seed = parse_list(value(&mut index)?)?,
                "--ports" => options.ports = parse_ports(value(&mut index)?)?,
                "--placement" => options.placement = parse_placement(value(&mut index)?)?,
                "--round-limit" => options.round_limit = Some(parse_value(value(&mut index)?)?),
                "--workers" => options.workers = parse_value(value(&mut index)?)?,
                "--help" | "-h" => {
                    print_help();
                    return Err(CliError::Help);
                }
                other => return Err(CliError::Usage(format!("unknown option {other:?}"))),
            }
            index += 1;
        }
        if options.nodes.is_empty() || options.agents.is_empty() || options.seed.is_empty() {
            return Err(CliError::Usage("sweep axes cannot be empty".to_owned()));
        }
        Ok(options)
    }
}

fn validate(options: &Options) -> Result<(), CliError> {
    let request = request(
        options,
        options.nodes[0],
        options.agents[0],
        options.seed[0],
    );
    let prepared =
        prepare_run(request).map_err(|error| CliError::Preparation(format!("{error:?}")))?;
    println!(
        "valid graph={} nodes={} edges={} agents={} seed={}",
        prepared.request.graph.label(),
        prepared.graph.node_count(),
        prepared.graph.edge_count(),
        prepared.request.agents,
        prepared.request.seed
    );
    Ok(())
}

fn run_plan(options: &Options) -> Result<(), CliError> {
    let request = request(
        options,
        options.nodes[0],
        options.agents[0],
        options.seed[0],
    );
    let result = execute_one(&BuiltinRunner, request)
        .map_err(|error| CliError::Preparation(error.to_string()))?;
    print!("{}", results_to_csv(&[result]));
    Ok(())
}

fn sweep_plan(options: &Options) -> Result<(), CliError> {
    let base = request(
        options,
        options.nodes[0],
        options.agents[0],
        options.seed[0],
    );
    let sweep = SweepSpec::new(base)
        .with_node_counts(options.nodes.clone())
        .with_agent_counts(options.agents.clone())
        .with_seeds(options.seed.clone());
    let results = run_sweep_parallel(&BuiltinRunner, &sweep, options.workers);
    let rows = results
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| CliError::Preparation(error.to_string()))?;
    print!("{}", results_to_csv(&rows));
    Ok(())
}

fn request(options: &Options, nodes: usize, agents: usize, seed: u64) -> RunRequest {
    RunRequest {
        algorithm: options.algorithm,
        graph: GraphSpec::new(options.graph.clone(), nodes),
        port_assignment: options.ports,
        placement: options.placement.clone(),
        agents,
        seed,
        round_limit: options.round_limit,
    }
}

fn parse_algorithm(value: &str) -> Result<Algorithm, CliError> {
    match value {
        "drop-and-freeze" | "drop_and_freeze" => Ok(Algorithm::DropAndFreeze),
        "help-by-scouts" | "help_by_scouts" => Ok(Algorithm::HelpByScouts),
        "p1tree" | "p1-tree" | "dfs-p1tree" => Ok(Algorithm::P1Tree),
        _ => Err(CliError::Parse(format!("unknown algorithm {value:?}"))),
    }
}

fn parse_graph(value: &str) -> Result<GraphFamily, CliError> {
    match value {
        "path" => Ok(GraphFamily::Path),
        "cycle" => Ok(GraphFamily::Cycle),
        "star" => Ok(GraphFamily::Star),
        "complete" => Ok(GraphFamily::Complete),
        "tree" => Ok(GraphFamily::Tree { branching: 2 }),
        "grid" => Ok(GraphFamily::Grid { columns: 3 }),
        value if value.starts_with("tree:") => Ok(GraphFamily::Tree {
            branching: parse_value(&value[5..])?,
        }),
        value if value.starts_with("grid:") => Ok(GraphFamily::Grid {
            columns: parse_value(&value[5..])?,
        }),
        value if value.starts_with("random-connected:") => Ok(GraphFamily::RandomConnected {
            extra_edges: parse_value(&value[17..])?,
        }),
        value if value.starts_with("random-bounded:") => Ok(GraphFamily::RandomBoundedDegree {
            max_degree: parse_value(&value[15..])?,
        }),
        _ => Err(CliError::Parse(format!("unknown graph {value:?}"))),
    }
}

fn parse_ports(value: &str) -> Result<PortAssignment, CliError> {
    match value {
        "canonical" => Ok(PortAssignment::Canonical),
        "random" => Ok(PortAssignment::Random),
        "adversarial" => Ok(PortAssignment::Adversarial),
        _ => Err(CliError::Parse(format!(
            "unknown port assignment {value:?}"
        ))),
    }
}

fn parse_placement(value: &str) -> Result<PlacementSpec, CliError> {
    match value {
        "single" => Ok(PlacementSpec::SingleNode { node: NodeId(0) }),
        value if value.starts_with("single:") => Ok(PlacementSpec::SingleNode {
            node: NodeId(parse_value(&value[7..])?),
        }),
        value if value.starts_with("uniform:") => Ok(PlacementSpec::UniformRandom {
            distinct_nodes: parse_value(&value[8..])?,
        }),
        value if value.starts_with("clustered:") => Ok(PlacementSpec::Clustered {
            centers: parse_nodes(&value[10..])?,
        }),
        value if value.starts_with("explicit:") => Ok(PlacementSpec::Explicit {
            nodes: parse_nodes(&value[9..])?,
        }),
        _ => Err(CliError::Parse(format!("unknown placement {value:?}"))),
    }
}

fn parse_nodes(value: &str) -> Result<Vec<NodeId>, CliError> {
    value
        .split(',')
        .map(|part| parse_value::<u32>(part).map(NodeId))
        .collect()
}

fn parse_list<T: std::str::FromStr>(value: &str) -> Result<Vec<T>, CliError>
where
    T::Err: fmt::Debug,
{
    value
        .split(',')
        .map(|part| {
            part.parse()
                .map_err(|error| CliError::Parse(format!("{error:?}")))
        })
        .collect()
}

fn parse_value<T: std::str::FromStr>(value: &str) -> Result<T, CliError>
where
    T::Err: fmt::Debug,
{
    value
        .parse()
        .map_err(|error| CliError::Parse(format!("invalid value {value:?}: {error:?}")))
}

fn print_help() {
    println!(
        "ccm commands:\n  validate [options]  generate and validate one deterministic run\n  run [options]       execute one run and emit CSV\n  sweep [options]     execute independent runs and emit CSV\n\noptions:\n  --algorithm drop-and-freeze|help-by-scouts|p1tree\n  --graph path|cycle|star|complete|tree[:branching]|grid[:columns]|random-connected:extra|random-bounded:max-degree\n  --nodes N[,N...] --agents A[,A...] --seed S[,S...]\n  --ports canonical|random|adversarial\n  --placement single[:node]|uniform:K|clustered:n,n|explicit:n,n\n  --round-limit R --workers W"
    );
}
