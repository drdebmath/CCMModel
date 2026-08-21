//! Import/export adapters for the JSON produced by `simulation_wrapper.py`.
//!
//! The legacy format has five positional histories whose meanings differ by
//! algorithm. This crate keeps that format at the boundary and converts it to
//! an explicit canonical representation. No legacy schema types are placed in
//! `ccm-core`.

use ccm_core::{AgentId, NodeId, PortId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Algorithm selector required to interpret the legacy five-slot histories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Algorithm {
    DropAndFreeze,
    HelpByScouts,
}

/// A legacy history item. The JSON representation is `[label, value]`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyFrame<T>(pub String, pub T);

/// Legacy graph node emitted by the Python wrapper.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LegacyNode {
    pub data: LegacyNodeData,
    pub position: LegacyPosition,
    pub classes: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyNodeData {
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LegacyPosition {
    pub x: f64,
    pub y: f64,
}

/// Ports in old output are normally integers, but invalid/debug output may
/// contain strings such as `"?"`; retaining the string lets validation return
/// a useful compatibility error instead of failing deserialization vaguely.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum LegacyPort {
    Number(i64),
    Text(String),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyEdge {
    pub data: LegacyEdgeData,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LegacyEdgeData {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "srcPort")]
    pub src_port: LegacyPort,
    #[serde(rename = "dstPort")]
    pub dst_port: LegacyPort,
}

/// The exact five positional fields emitted by the wrapper.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LegacyEnvelope {
    pub nodes: Vec<LegacyNode>,
    pub edges: Vec<LegacyEdge>,
    pub positions: Vec<LegacyFrame<Value>>,
    pub statuses: Vec<LegacyFrame<Value>>,
    #[serde(rename = "node_settled_states")]
    pub node_settled_states: Vec<LegacyFrame<Value>>,
    pub homes: Vec<LegacyFrame<Value>>,
    #[serde(rename = "tree_edges")]
    pub tree_edges: Vec<LegacyFrame<Value>>,
}

/// Canonical status names shared by both algorithm adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalStatus {
    Settled,
    Unsettled,
    SettledWaiting,
    SettledScout,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalNode {
    pub id: NodeId,
    pub x: f64,
    pub y: f64,
    pub classes: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalEdge {
    pub id: usize,
    pub source: NodeId,
    pub target: NodeId,
    pub source_port: PortId,
    pub target_port: PortId,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalGraph {
    pub nodes: Vec<CanonicalNode>,
    pub edges: Vec<CanonicalEdge>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalNodeState {
    pub settled_agent: Option<AgentId>,
    pub parent_port: Option<PortId>,
    pub checked_port: Option<PortId>,
    pub max_scouted_port: Option<PortId>,
    pub next_port: Option<PortId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalTreeEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub source_port: PortId,
    pub target_port: PortId,
}

/// Explicit algorithm-specific state for one canonical frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalAlgorithmState {
    DropAndFreeze {
        leaders: Vec<AgentId>,
        levels: Vec<u64>,
        node_states: BTreeMap<NodeId, Option<CanonicalNodeState>>,
    },
    HelpByScouts {
        node_states: Vec<CanonicalNodeState>,
        homes: Vec<Option<NodeId>>,
        tree_edges: Vec<CanonicalTreeEdge>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalFrame {
    pub label: String,
    pub positions: Vec<NodeId>,
    pub statuses: Vec<CanonicalStatus>,
    pub algorithm_state: CanonicalAlgorithmState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalTrace {
    pub frames: Vec<CanonicalFrame>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalResult {
    pub algorithm: Algorithm,
    pub graph: CanonicalGraph,
    pub trace: CanonicalTrace,
}

impl CanonicalResult {
    #[must_use]
    pub fn final_frame(&self) -> Option<&CanonicalFrame> {
        self.trace.frames.last()
    }
}

/// Errors raised while decoding, validating, or exporting compatibility data.
#[derive(Debug)]
pub enum CompatibilityError {
    Json(serde_json::Error),
    Invalid(String),
}

impl fmt::Display for CompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(f, "legacy JSON error: {error}"),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CompatibilityError {}

impl From<serde_json::Error> for CompatibilityError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

/// Imports and validates one legacy wrapper JSON document.
///
/// # Errors
///
/// Returns [`CompatibilityError`] for malformed JSON or any graph, port,
/// history-length, status, or state validation failure.
pub fn import_legacy_json(
    input: &str,
    algorithm: Algorithm,
) -> Result<CanonicalResult, CompatibilityError> {
    let envelope: LegacyEnvelope = serde_json::from_str(input)?;
    import_legacy(&envelope, algorithm)
}

/// Imports and validates a typed legacy envelope.
///
/// # Errors
///
/// Returns [`CompatibilityError`] when graph topology, ports, history labels,
/// vector lengths, IDs, or algorithm-specific slots are invalid.
pub fn import_legacy(
    envelope: &LegacyEnvelope,
    algorithm: Algorithm,
) -> Result<CanonicalResult, CompatibilityError> {
    let graph = parse_graph(envelope)?;
    let positions = parse_position_history(&envelope.positions, algorithm)?;
    let statuses = parse_status_history(&envelope.statuses, algorithm)?;
    if positions.len() != statuses.len() {
        return Err(invalid(format!(
            "positions/statuses trace length mismatch: {} != {}",
            positions.len(),
            statuses.len()
        )));
    }

    let state_histories = match algorithm {
        Algorithm::DropAndFreeze => {
            let leaders = parse_agent_id_history(&envelope.node_settled_states, "leaders")?;
            let levels = parse_u64_history(&envelope.homes, "levels")?;
            let node_states = parse_drop_node_state_history(&envelope.tree_edges)?;
            if leaders.len() != positions.len()
                || levels.len() != positions.len()
                || node_states.len() != positions.len()
            {
                return Err(invalid("Drop-and-Freeze five-slot trace lengths differ"));
            }
            DropHistory {
                leaders,
                levels,
                node_states,
            }
            .into_state_histories()?
        }
        Algorithm::HelpByScouts => {
            let node_states = parse_help_node_state_history(&envelope.node_settled_states)?;
            let homes = parse_home_history(&envelope.homes)?;
            let tree_edges = parse_tree_edge_history(&envelope.tree_edges)?;
            if node_states.len() != positions.len()
                || homes.len() != positions.len()
                || tree_edges.len() != positions.len()
            {
                return Err(invalid("Help-by-Scouts five-slot trace lengths differ"));
            }
            HelpHistory {
                node_states,
                homes,
                tree_edges,
            }
            .into_state_histories()?
        }
    };

    let agent_count = positions.first().map_or(0, |(_, value)| value.len());
    let mut frames = Vec::with_capacity(positions.len());
    for index in 0..positions.len() {
        let (label, position_values) = &positions[index];
        let (status_label, status_values) = &statuses[index];
        if label != status_label || label != &state_histories[index].label() {
            return Err(invalid(format!("trace label mismatch at frame {index}")));
        }
        if position_values.len() != agent_count || status_values.len() != agent_count {
            return Err(invalid(format!(
                "agent vector length mismatch at frame {index}"
            )));
        }
        for &node in position_values {
            require_node(&graph, node, "position")?;
        }
        let state = state_histories[index].canonical_state(agent_count)?;
        validate_algorithm_state(&graph, agent_count, &state)?;
        frames.push(CanonicalFrame {
            label: label.clone(),
            positions: position_values.clone(),
            statuses: status_values.clone(),
            algorithm_state: state,
        });
    }
    Ok(CanonicalResult {
        algorithm,
        graph,
        trace: CanonicalTrace { frames },
    })
}

/// Exports a canonical result using the legacy wrapper's five slots.
///
/// # Errors
///
/// Returns [`CompatibilityError`] when a frame's algorithm-specific state does
/// not match the result's selected algorithm or JSON serialization fails.
pub fn export_legacy_json(result: &CanonicalResult) -> Result<String, CompatibilityError> {
    Ok(serde_json::to_string_pretty(&export_legacy(result)?)?)
}

/// Exports a typed canonical result to the legacy envelope. The algorithm
/// selector determines the historical slot mismatch on purpose.
///
/// # Errors
///
/// Returns [`CompatibilityError`] when a frame's algorithm-specific state does
/// not match the result's selected algorithm.
#[allow(clippy::too_many_lines)]
pub fn export_legacy(result: &CanonicalResult) -> Result<LegacyEnvelope, CompatibilityError> {
    let mut positions = Vec::with_capacity(result.trace.frames.len());
    let mut statuses = Vec::with_capacity(result.trace.frames.len());
    let mut node_settled_states = Vec::with_capacity(result.trace.frames.len());
    let mut homes = Vec::with_capacity(result.trace.frames.len());
    let mut tree_edges = Vec::with_capacity(result.trace.frames.len());

    for frame in &result.trace.frames {
        positions.push(LegacyFrame(
            frame.label.clone(),
            encode_positions(&frame.positions, result.algorithm),
        ));
        statuses.push(LegacyFrame(
            frame.label.clone(),
            encode_statuses(&frame.statuses, result.algorithm),
        ));
        match &frame.algorithm_state {
            CanonicalAlgorithmState::DropAndFreeze {
                leaders,
                levels,
                node_states,
            } => {
                if result.algorithm != Algorithm::DropAndFreeze {
                    return Err(invalid(
                        "frame algorithm state does not match result algorithm",
                    ));
                }
                node_settled_states.push(LegacyFrame(
                    frame.label.clone(),
                    Value::Array(leaders.iter().map(|id| Value::from(id.0)).collect()),
                ));
                homes.push(LegacyFrame(
                    frame.label.clone(),
                    Value::Array(levels.iter().map(|level| Value::from(*level)).collect()),
                ));
                tree_edges.push(LegacyFrame(
                    frame.label.clone(),
                    encode_drop_node_states(node_states),
                ));
            }
            CanonicalAlgorithmState::HelpByScouts {
                node_states,
                homes: frame_homes,
                tree_edges: frame_tree_edges,
            } => {
                if result.algorithm != Algorithm::HelpByScouts {
                    return Err(invalid(
                        "frame algorithm state does not match result algorithm",
                    ));
                }
                node_settled_states.push(LegacyFrame(
                    frame.label.clone(),
                    Value::Array(
                        node_states
                            .iter()
                            .map(encode_node_state)
                            .collect::<Vec<_>>(),
                    ),
                ));
                homes.push(LegacyFrame(
                    frame.label.clone(),
                    Value::Array(
                        frame_homes
                            .iter()
                            .map(|home| match home {
                                Some(node) => Value::String(node.0.to_string()),
                                None => Value::String("None".to_owned()),
                            })
                            .map(|value| Value::Array(vec![value]))
                            .collect(),
                    ),
                ));
                tree_edges.push(LegacyFrame(
                    frame.label.clone(),
                    Value::Array(
                        frame_tree_edges
                            .iter()
                            .map(encode_tree_edge)
                            .collect::<Vec<_>>(),
                    ),
                ));
            }
        }
    }

    Ok(LegacyEnvelope {
        nodes: result
            .graph
            .nodes
            .iter()
            .map(|node| LegacyNode {
                data: LegacyNodeData {
                    id: node.id.0.to_string(),
                },
                position: LegacyPosition {
                    x: node.x,
                    y: node.y,
                },
                classes: node.classes.clone(),
            })
            .collect(),
        edges: result
            .graph
            .edges
            .iter()
            .map(|edge| LegacyEdge {
                data: LegacyEdgeData {
                    id: edge.id.to_string(),
                    source: edge.source.0.to_string(),
                    target: edge.target.0.to_string(),
                    src_port: LegacyPort::Number(i64::from(edge.source_port.0)),
                    dst_port: LegacyPort::Number(i64::from(edge.target_port.0)),
                },
            })
            .collect(),
        positions,
        statuses,
        node_settled_states,
        homes,
        tree_edges,
    })
}

struct DropHistory {
    leaders: Vec<(String, Vec<AgentId>)>,
    levels: Vec<(String, Vec<u64>)>,
    node_states: Vec<(String, BTreeMap<NodeId, Option<CanonicalNodeState>>)>,
}

impl DropHistory {
    fn into_state_histories(self) -> Result<Vec<StateHistory>, CompatibilityError> {
        if self
            .leaders
            .iter()
            .zip(&self.levels)
            .zip(&self.node_states)
            .any(|((leaders, levels), node_states)| {
                leaders.0 != levels.0 || leaders.0 != node_states.0
            })
        {
            return Err(invalid("Drop-and-Freeze state history labels differ"));
        }
        Ok(self
            .leaders
            .into_iter()
            .zip(self.levels)
            .zip(self.node_states)
            .map(|((leaders, levels), node_states)| StateHistory::Drop {
                label: leaders.0,
                leaders: leaders.1,
                levels: levels.1,
                node_states: node_states.1,
            })
            .collect::<Vec<_>>())
    }
}

struct HelpHistory {
    node_states: Vec<(String, Vec<CanonicalNodeState>)>,
    homes: Vec<(String, Vec<Option<NodeId>>)>,
    tree_edges: Vec<(String, Vec<CanonicalTreeEdge>)>,
}

type LabeledNodeStates = Vec<(String, BTreeMap<NodeId, Option<CanonicalNodeState>>)>;
type LabeledHomes = Vec<(String, Vec<Option<NodeId>>)>;

impl HelpHistory {
    fn into_state_histories(self) -> Result<Vec<StateHistory>, CompatibilityError> {
        if self
            .node_states
            .iter()
            .zip(&self.homes)
            .zip(&self.tree_edges)
            .any(|((node_states, homes), tree_edges)| {
                node_states.0 != homes.0 || node_states.0 != tree_edges.0
            })
        {
            return Err(invalid("Help-by-Scouts state history labels differ"));
        }
        Ok(self
            .node_states
            .into_iter()
            .zip(self.homes)
            .zip(self.tree_edges)
            .map(|((node_states, homes), tree_edges)| StateHistory::Help {
                label: node_states.0,
                node_states: node_states.1,
                homes: homes.1,
                tree_edges: tree_edges.1,
            })
            .collect::<Vec<_>>())
    }
}

#[derive(Clone)]
enum StateHistory {
    Drop {
        label: String,
        leaders: Vec<AgentId>,
        levels: Vec<u64>,
        node_states: BTreeMap<NodeId, Option<CanonicalNodeState>>,
    },
    Help {
        label: String,
        node_states: Vec<CanonicalNodeState>,
        homes: Vec<Option<NodeId>>,
        tree_edges: Vec<CanonicalTreeEdge>,
    },
}

impl StateHistory {
    fn label(&self) -> String {
        match self {
            Self::Drop { label, .. } | Self::Help { label, .. } => label.clone(),
        }
    }

    fn canonical_state(
        &self,
        agent_count: usize,
    ) -> Result<CanonicalAlgorithmState, CompatibilityError> {
        match self {
            Self::Drop {
                leaders,
                levels,
                node_states,
                ..
            } => {
                if leaders.len() != agent_count || levels.len() != agent_count {
                    return Err(invalid("Drop-and-Freeze state vector length mismatch"));
                }
                Ok(CanonicalAlgorithmState::DropAndFreeze {
                    leaders: leaders.clone(),
                    levels: levels.clone(),
                    node_states: node_states.clone(),
                })
            }
            Self::Help {
                node_states,
                homes,
                tree_edges,
                ..
            } => {
                if homes.len() != agent_count {
                    return Err(invalid("Help-by-Scouts homes vector length mismatch"));
                }
                Ok(CanonicalAlgorithmState::HelpByScouts {
                    node_states: node_states.clone(),
                    homes: homes.clone(),
                    tree_edges: tree_edges.clone(),
                })
            }
        }
    }
}

fn parse_graph(envelope: &LegacyEnvelope) -> Result<CanonicalGraph, CompatibilityError> {
    let mut nodes = Vec::with_capacity(envelope.nodes.len());
    let mut node_ids = BTreeSet::new();
    for (index, node) in envelope.nodes.iter().enumerate() {
        let id = parse_node_text(&node.data.id, &format!("nodes[{index}].data.id"))?;
        if !node_ids.insert(id) {
            return Err(invalid(format!("duplicate graph node {id:?}")));
        }
        if !node.position.x.is_finite() || !node.position.y.is_finite() {
            return Err(invalid(format!("non-finite position for node {id:?}")));
        }
        nodes.push(CanonicalNode {
            id,
            x: node.position.x,
            y: node.position.y,
            classes: node.classes.clone(),
        });
    }

    let mut edges = Vec::with_capacity(envelope.edges.len());
    let mut edge_ids = BTreeSet::new();
    let mut undirected_pairs = BTreeSet::new();
    let mut local_ports = BTreeMap::new();
    for (index, edge) in envelope.edges.iter().enumerate() {
        let data = &edge.data;
        let source = parse_node_text(&data.source, &format!("edges[{index}].data.source"))?;
        let target = parse_node_text(&data.target, &format!("edges[{index}].data.target"))?;
        require_node_ids(&node_ids, source, "edge source")?;
        require_node_ids(&node_ids, target, "edge target")?;
        if source == target {
            return Err(invalid(format!("self-loop edge {}", data.id)));
        }
        if !edge_ids.insert(data.id.clone()) {
            return Err(invalid(format!("duplicate edge id {}", data.id)));
        }
        let pair = if source < target {
            (source, target)
        } else {
            (target, source)
        };
        if !undirected_pairs.insert(pair) {
            return Err(invalid(format!(
                "duplicate undirected edge {source:?}-{target:?}"
            )));
        }
        let source_port = parse_port(&data.src_port, &format!("edges[{index}].data.srcPort"))?;
        let target_port = parse_port(&data.dst_port, &format!("edges[{index}].data.dstPort"))?;
        if local_ports.insert((source, source_port), target).is_some()
            || local_ports.insert((target, target_port), source).is_some()
        {
            return Err(invalid(format!("duplicate local port on edge {}", data.id)));
        }
        edges.push(CanonicalEdge {
            id: index,
            source,
            target,
            source_port,
            target_port,
        });
    }
    for &node in &node_ids {
        let degree = local_ports
            .keys()
            .filter(|(source, _)| *source == node)
            .count();
        for port in 0..degree {
            let port = PortId(
                u16::try_from(port)
                    .map_err(|_| invalid(format!("node {node:?} has too many ports")))?,
            );
            if !local_ports.contains_key(&(node, port)) {
                return Err(invalid(format!(
                    "node {node:?} ports must be contiguous from zero"
                )));
            }
        }
    }
    Ok(CanonicalGraph { nodes, edges })
}

fn parse_position_history(
    history: &[LegacyFrame<Value>],
    algorithm: Algorithm,
) -> Result<Vec<(String, Vec<NodeId>)>, CompatibilityError> {
    history
        .iter()
        .map(|frame| {
            Ok((
                frame.0.clone(),
                parse_node_vector(&frame.1, algorithm, "positions")?,
            ))
        })
        .collect()
}

fn parse_status_history(
    history: &[LegacyFrame<Value>],
    algorithm: Algorithm,
) -> Result<Vec<(String, Vec<CanonicalStatus>)>, CompatibilityError> {
    history
        .iter()
        .map(|frame| {
            Ok((
                frame.0.clone(),
                parse_status_vector(&frame.1, algorithm, "statuses")?,
            ))
        })
        .collect()
}

fn parse_agent_id_history(
    history: &[LegacyFrame<Value>],
    name: &str,
) -> Result<Vec<(String, Vec<AgentId>)>, CompatibilityError> {
    history
        .iter()
        .map(|frame| Ok((frame.0.clone(), parse_agent_vector(&frame.1, name)?)))
        .collect()
}

fn parse_u64_history(
    history: &[LegacyFrame<Value>],
    name: &str,
) -> Result<Vec<(String, Vec<u64>)>, CompatibilityError> {
    history
        .iter()
        .map(|frame| Ok((frame.0.clone(), parse_u64_vector(&frame.1, name)?)))
        .collect()
}

fn parse_drop_node_state_history(
    history: &[LegacyFrame<Value>],
) -> Result<LabeledNodeStates, CompatibilityError> {
    history
        .iter()
        .map(|frame| Ok((frame.0.clone(), parse_drop_node_states(&frame.1)?)))
        .collect()
}

fn parse_help_node_state_history(
    history: &[LegacyFrame<Value>],
) -> Result<Vec<(String, Vec<CanonicalNodeState>)>, CompatibilityError> {
    history
        .iter()
        .map(|frame| {
            let values = frame
                .1
                .as_array()
                .ok_or_else(|| invalid(format!("{} node states must be an array", frame.0)))?;
            let states = values
                .iter()
                .map(parse_node_state)
                .collect::<Result<Vec<_>, _>>()?;
            Ok((frame.0.clone(), states))
        })
        .collect()
}

fn parse_home_history(history: &[LegacyFrame<Value>]) -> Result<LabeledHomes, CompatibilityError> {
    history
        .iter()
        .map(|frame| Ok((frame.0.clone(), parse_home_vector(&frame.1)?)))
        .collect()
}

fn parse_tree_edge_history(
    history: &[LegacyFrame<Value>],
) -> Result<Vec<(String, Vec<CanonicalTreeEdge>)>, CompatibilityError> {
    history
        .iter()
        .map(|frame| {
            let values = frame
                .1
                .as_array()
                .ok_or_else(|| invalid(format!("{} tree edges must be an array", frame.0)))?;
            let edges = values
                .iter()
                .map(parse_tree_edge)
                .collect::<Result<Vec<_>, _>>()?;
            Ok((frame.0.clone(), edges))
        })
        .collect()
}

fn parse_node_vector(
    value: &Value,
    algorithm: Algorithm,
    name: &str,
) -> Result<Vec<NodeId>, CompatibilityError> {
    let values = value
        .as_array()
        .ok_or_else(|| invalid(format!("{name} value must be an array")))?;
    values
        .iter()
        .map(|item| {
            let item = if algorithm == Algorithm::HelpByScouts {
                singleton(item, name)?
            } else {
                item
            };
            parse_node_value(item, name)
        })
        .collect()
}

fn parse_status_vector(
    value: &Value,
    algorithm: Algorithm,
    name: &str,
) -> Result<Vec<CanonicalStatus>, CompatibilityError> {
    let values = value
        .as_array()
        .ok_or_else(|| invalid(format!("{name} value must be an array")))?;
    values
        .iter()
        .map(|item| {
            let item = if algorithm == Algorithm::HelpByScouts {
                singleton(item, name)?
            } else {
                item
            };
            parse_status_value(item, algorithm, name)
        })
        .collect()
}

fn parse_agent_vector(value: &Value, name: &str) -> Result<Vec<AgentId>, CompatibilityError> {
    value
        .as_array()
        .ok_or_else(|| invalid(format!("{name} value must be an array")))?
        .iter()
        .map(|item| parse_agent_value(item, name))
        .collect()
}

fn parse_u64_vector(value: &Value, name: &str) -> Result<Vec<u64>, CompatibilityError> {
    value
        .as_array()
        .ok_or_else(|| invalid(format!("{name} value must be an array")))?
        .iter()
        .map(|item| parse_u64_value(item, name))
        .collect()
}

fn parse_home_vector(value: &Value) -> Result<Vec<Option<NodeId>>, CompatibilityError> {
    value
        .as_array()
        .ok_or_else(|| invalid("homes value must be an array"))?
        .iter()
        .map(|item| {
            let item = singleton(item, "homes")?;
            if item.as_str() == Some("None") || item.is_null() {
                Ok(None)
            } else {
                parse_node_value(item, "homes").map(Some)
            }
        })
        .collect()
}

fn parse_drop_node_states(
    value: &Value,
) -> Result<BTreeMap<NodeId, Option<CanonicalNodeState>>, CompatibilityError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("Drop-and-Freeze node states must be an object"))?;
    object
        .iter()
        .map(|(key, value)| {
            let node = parse_node_text(key, "node state key")?;
            let state = if value.is_null() {
                None
            } else {
                Some(parse_node_state(value)?)
            };
            Ok((node, state))
        })
        .collect()
}

fn parse_node_state(value: &Value) -> Result<CanonicalNodeState, CompatibilityError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("node state must be an object"))?;
    Ok(CanonicalNodeState {
        settled_agent: optional_agent(object, "settled_agent_id")?,
        parent_port: optional_port(object, "parent_port")?,
        checked_port: optional_port(object, "checked_port")?,
        max_scouted_port: optional_port(object, "max_scouted_port")?,
        next_port: optional_port(object, "next_port")?,
    })
}

fn parse_tree_edge(value: &Value) -> Result<CanonicalTreeEdge, CompatibilityError> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("tree edge must be an object"))?;
    let source = required_string(object, "u")?;
    let target = required_string(object, "v")?;
    Ok(CanonicalTreeEdge {
        source: parse_node_text(source, "tree edge u")?,
        target: parse_node_text(target, "tree edge v")?,
        source_port: parse_port_value(object.get("srcPort"), "tree edge srcPort")?,
        target_port: parse_port_value(object.get("dstPort"), "tree edge dstPort")?,
    })
}

fn parse_node_value(value: &Value, name: &str) -> Result<NodeId, CompatibilityError> {
    parse_u32_value(value, name).map(NodeId)
}

fn parse_agent_value(value: &Value, name: &str) -> Result<AgentId, CompatibilityError> {
    parse_u32_value(value, name).map(AgentId)
}

fn parse_u64_value(value: &Value, name: &str) -> Result<u64, CompatibilityError> {
    if let Some(number) = value.as_u64() {
        return Ok(number);
    }
    if let Some(text) = value.as_str() {
        return text
            .parse::<u64>()
            .map_err(|_| invalid(format!("{name} is not a nonnegative integer")));
    }
    Err(invalid(format!("{name} is not an integer")))
}

fn parse_u32_value(value: &Value, name: &str) -> Result<u32, CompatibilityError> {
    let number = parse_u64_value(value, name)?;
    u32::try_from(number).map_err(|_| invalid(format!("{name} exceeds u32")))
}

fn parse_node_text(text: &str, name: &str) -> Result<NodeId, CompatibilityError> {
    text.parse::<u32>()
        .map(NodeId)
        .map_err(|_| invalid(format!("{name} is not a dense numeric node ID: {text}")))
}

fn parse_port(port: &LegacyPort, name: &str) -> Result<PortId, CompatibilityError> {
    match port {
        LegacyPort::Number(number) => u16::try_from(*number)
            .map(PortId)
            .map_err(|_| invalid(format!("{name} is outside the PortId range"))),
        LegacyPort::Text(text) => Err(invalid(format!("{name} is not numeric: {text}"))),
    }
}

fn parse_port_value(value: Option<&Value>, name: &str) -> Result<PortId, CompatibilityError> {
    let value = value.ok_or_else(|| invalid(format!("missing {name}")))?;
    if let Some(number) = value.as_i64() {
        return parse_port(&LegacyPort::Number(number), name);
    }
    Err(invalid(format!("{name} is not numeric")))
}

fn parse_status_value(
    value: &Value,
    algorithm: Algorithm,
    name: &str,
) -> Result<CanonicalStatus, CompatibilityError> {
    match algorithm {
        Algorithm::DropAndFreeze => match parse_u64_value(value, name)? {
            0 => Ok(CanonicalStatus::Settled),
            1 => Ok(CanonicalStatus::Unsettled),
            2 => Ok(CanonicalStatus::SettledWaiting),
            value => Err(invalid(format!("unknown Drop-and-Freeze status {value}"))),
        },
        Algorithm::HelpByScouts => match value.as_str() {
            Some("settled") => Ok(CanonicalStatus::Settled),
            Some("unsettled") => Ok(CanonicalStatus::Unsettled),
            Some("settledScout") => Ok(CanonicalStatus::SettledScout),
            Some(value) => Err(invalid(format!("unknown Help-by-Scouts status {value}"))),
            None => Err(invalid(format!("{name} status is not a string"))),
        },
    }
}

fn singleton<'a>(value: &'a Value, name: &str) -> Result<&'a Value, CompatibilityError> {
    let values = value.as_array().ok_or_else(|| {
        invalid(format!(
            "{name} Help-by-Scouts item must be a one-element array"
        ))
    })?;
    if values.len() != 1 {
        return Err(invalid(format!("{name} item must have one element")));
    }
    Ok(&values[0])
}

fn optional_agent(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<AgentId>, CompatibilityError> {
    object
        .get(key)
        .filter(|value| !value.is_null())
        .map(|value| parse_agent_value(value, key))
        .transpose()
}

fn optional_port(
    object: &Map<String, Value>,
    key: &str,
) -> Result<Option<PortId>, CompatibilityError> {
    object
        .get(key)
        .filter(|value| !value.is_null())
        .map(|value| parse_port_value(Some(value), key))
        .transpose()
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, CompatibilityError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("missing string field {key}")))
}

fn validate_algorithm_state(
    graph: &CanonicalGraph,
    agent_count: usize,
    state: &CanonicalAlgorithmState,
) -> Result<(), CompatibilityError> {
    match state {
        CanonicalAlgorithmState::DropAndFreeze {
            leaders,
            levels,
            node_states,
        } => {
            if leaders.len() != agent_count || levels.len() != agent_count {
                return Err(invalid("Drop-and-Freeze state length mismatch"));
            }
            for &leader in leaders {
                require_agent(leader, agent_count)?;
            }
            for (&node, state) in node_states {
                require_node(graph, node, "node state")?;
                if let Some(state) = state {
                    validate_node_state(graph, agent_count, node, state)?;
                }
            }
        }
        CanonicalAlgorithmState::HelpByScouts {
            node_states,
            homes,
            tree_edges,
        } => {
            if homes.len() != agent_count {
                return Err(invalid("Help-by-Scouts homes length mismatch"));
            }
            for &home in homes.iter().flatten() {
                require_node(graph, home, "home")?;
            }
            for state in node_states {
                if let Some(agent) = state.settled_agent {
                    require_agent(agent, agent_count)?;
                }
            }
            for edge in tree_edges {
                validate_tree_edge(graph, edge)?;
            }
        }
    }
    Ok(())
}

fn validate_node_state(
    graph: &CanonicalGraph,
    agent_count: usize,
    node: NodeId,
    state: &CanonicalNodeState,
) -> Result<(), CompatibilityError> {
    if let Some(agent) = state.settled_agent {
        require_agent(agent, agent_count)?;
    }
    for port in [
        state.parent_port,
        state.checked_port,
        state.max_scouted_port,
        state.next_port,
    ]
    .into_iter()
    .flatten()
    {
        require_port(graph, node, port)?;
    }
    Ok(())
}

fn validate_tree_edge(
    graph: &CanonicalGraph,
    edge: &CanonicalTreeEdge,
) -> Result<(), CompatibilityError> {
    let target = graph.edges.iter().find(|candidate| {
        (candidate.source == edge.source
            && candidate.target == edge.target
            && candidate.source_port == edge.source_port
            && candidate.target_port == edge.target_port)
            || (candidate.source == edge.target
                && candidate.target == edge.source
                && candidate.source_port == edge.target_port
                && candidate.target_port == edge.source_port)
    });
    if target.is_none() {
        return Err(invalid(format!(
            "tree edge does not match a graph edge: {edge:?}"
        )));
    }
    Ok(())
}

fn require_node(
    graph: &CanonicalGraph,
    node: NodeId,
    role: &str,
) -> Result<(), CompatibilityError> {
    if graph.nodes.iter().any(|candidate| candidate.id == node) {
        Ok(())
    } else {
        Err(invalid(format!("{role} references unknown node {node:?}")))
    }
}

fn require_node_ids(
    nodes: &BTreeSet<NodeId>,
    node: NodeId,
    role: &str,
) -> Result<(), CompatibilityError> {
    if nodes.contains(&node) {
        Ok(())
    } else {
        Err(invalid(format!("{role} references unknown node {node:?}")))
    }
}

fn require_agent(agent: AgentId, agent_count: usize) -> Result<(), CompatibilityError> {
    if agent.index() < agent_count {
        Ok(())
    } else {
        Err(invalid(format!("references unknown agent {agent:?}")))
    }
}

fn require_port(
    graph: &CanonicalGraph,
    node: NodeId,
    port: PortId,
) -> Result<(), CompatibilityError> {
    let exists = graph.edges.iter().any(|edge| {
        (edge.source == node && edge.source_port == port)
            || (edge.target == node && edge.target_port == port)
    });
    if exists {
        Ok(())
    } else {
        Err(invalid(format!("node {node:?} has no port {port:?}")))
    }
}

fn encode_positions(positions: &[NodeId], algorithm: Algorithm) -> Value {
    match algorithm {
        Algorithm::DropAndFreeze => {
            Value::Array(positions.iter().map(|node| Value::from(node.0)).collect())
        }
        Algorithm::HelpByScouts => Value::Array(
            positions
                .iter()
                .map(|node| Value::Array(vec![Value::String(node.0.to_string())]))
                .collect(),
        ),
    }
}

fn encode_statuses(statuses: &[CanonicalStatus], algorithm: Algorithm) -> Value {
    Value::Array(
        statuses
            .iter()
            .map(|status| match algorithm {
                Algorithm::DropAndFreeze => Value::from(match status {
                    CanonicalStatus::Settled => 0,
                    CanonicalStatus::Unsettled | CanonicalStatus::SettledScout => 1,
                    CanonicalStatus::SettledWaiting => 2,
                }),
                Algorithm::HelpByScouts => Value::Array(vec![Value::String(
                    match status {
                        CanonicalStatus::Settled => "settled",
                        CanonicalStatus::Unsettled | CanonicalStatus::SettledWaiting => "unsettled",
                        CanonicalStatus::SettledScout => "settledScout",
                    }
                    .to_owned(),
                )]),
            })
            .collect(),
    )
}

fn encode_drop_node_states(states: &BTreeMap<NodeId, Option<CanonicalNodeState>>) -> Value {
    let mut object = Map::new();
    for (node, state) in states {
        object.insert(
            node.0.to_string(),
            state.as_ref().map_or(Value::Null, encode_node_state),
        );
    }
    Value::Object(object)
}

fn encode_node_state(state: &CanonicalNodeState) -> Value {
    let mut object = Map::new();
    object.insert(
        "settled_agent_id".to_owned(),
        state
            .settled_agent
            .map_or(Value::Null, |agent| Value::from(agent.0)),
    );
    object.insert(
        "parent_port".to_owned(),
        encode_optional_port(state.parent_port),
    );
    object.insert(
        "checked_port".to_owned(),
        encode_optional_port(state.checked_port),
    );
    object.insert(
        "max_scouted_port".to_owned(),
        encode_optional_port(state.max_scouted_port),
    );
    object.insert(
        "next_port".to_owned(),
        encode_optional_port(state.next_port),
    );
    Value::Object(object)
}

fn encode_optional_port(port: Option<PortId>) -> Value {
    port.map_or(Value::Null, |port| Value::from(port.0))
}

fn encode_tree_edge(edge: &CanonicalTreeEdge) -> Value {
    let mut object = Map::new();
    object.insert("u".to_owned(), Value::String(edge.source.0.to_string()));
    object.insert("v".to_owned(), Value::String(edge.target.0.to_string()));
    object.insert("srcPort".to_owned(), Value::from(edge.source_port.0));
    object.insert("dstPort".to_owned(), Value::from(edge.target_port.0));
    Value::Object(object)
}

fn invalid(message: impl Into<String>) -> CompatibilityError {
    CompatibilityError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_json() -> &'static str {
        r#"{
          "nodes": [
            {"data":{"id":"0"},"position":{"x":0.0,"y":0.0},"classes":"graph-node"},
            {"data":{"id":"1"},"position":{"x":1.0,"y":0.0},"classes":"graph-node"}
          ],
          "edges": [
            {"data":{"id":"0-1","source":"0","target":"1","srcPort":0,"dstPort":0}}
          ],
          "positions": [["start",[0,0]]],
          "statuses": [["start",[1,1]]],
          "node_settled_states": [["start",[0,0]]],
          "homes": [["start",[0,0]]],
          "tree_edges": [["start",{"0":null,"1":{"settled_agent_id":1,"parent_port":null,"checked_port":null,"max_scouted_port":null,"next_port":null}}]]
        }"#
    }

    fn help_json() -> &'static str {
        r#"{
          "nodes": [
            {"data":{"id":"0"},"position":{"x":0.0,"y":0.0},"classes":"graph-node"},
            {"data":{"id":"1"},"position":{"x":1.0,"y":0.0},"classes":"graph-node"}
          ],
          "edges": [
            {"data":{"id":"0-1","source":"0","target":"1","srcPort":0,"dstPort":0}}
          ],
          "positions": [["rooted_async:exit",[["1"],["0"]]]],
          "statuses": [["rooted_async:exit",[["settled"],["settled"]]]],
          "node_settled_states": [["rooted_async:exit",[]]],
          "homes": [["rooted_async:exit",[["1"],["0"]]]],
          "tree_edges": [["rooted_async:exit",[{"u":"1","v":"0","srcPort":0,"dstPort":0}]]]
        }"#
    }

    #[test]
    fn imports_drop_slot_mismatch_as_named_state() {
        let result = import_legacy_json(graph_json(), Algorithm::DropAndFreeze).unwrap();
        let frame = result.final_frame().unwrap();
        assert_eq!(frame.positions, vec![NodeId(0), NodeId(0)]);
        match &frame.algorithm_state {
            CanonicalAlgorithmState::DropAndFreeze {
                leaders,
                levels,
                node_states,
            } => {
                assert_eq!(leaders, &vec![AgentId(0), AgentId(0)]);
                assert_eq!(levels, &vec![0, 0]);
                assert_eq!(
                    node_states.get(&NodeId(1)).unwrap().unwrap().settled_agent,
                    Some(AgentId(1))
                );
            }
            CanonicalAlgorithmState::HelpByScouts { .. } => panic!("wrong algorithm state"),
        }
    }

    #[test]
    fn imports_help_slots_and_exports_them_without_relabeling() {
        let result = import_legacy_json(help_json(), Algorithm::HelpByScouts).unwrap();
        let frame = result.final_frame().unwrap();
        match &frame.algorithm_state {
            CanonicalAlgorithmState::HelpByScouts {
                homes,
                tree_edges,
                node_states,
            } => {
                assert_eq!(homes, &vec![Some(NodeId(1)), Some(NodeId(0))]);
                assert_eq!(tree_edges.len(), 1);
                assert!(node_states.is_empty());
            }
            CanonicalAlgorithmState::DropAndFreeze { .. } => panic!("wrong algorithm state"),
        }
        let exported = export_legacy_json(&result).unwrap();
        let reparsed = import_legacy_json(&exported, Algorithm::HelpByScouts).unwrap();
        assert_eq!(reparsed, result);
    }

    #[test]
    fn rejects_invalid_ports_and_trace_lengths() {
        let invalid_port = graph_json().replace("\"srcPort\":0", "\"srcPort\":9");
        assert!(matches!(
            import_legacy_json(&invalid_port, Algorithm::DropAndFreeze),
            Err(CompatibilityError::Invalid(_))
        ));
        let invalid_length =
            graph_json().replace("\"homes\": [[\"start\",[0,0]]]", "\"homes\": []");
        assert!(matches!(
            import_legacy_json(&invalid_length, Algorithm::DropAndFreeze),
            Err(CompatibilityError::Invalid(_))
        ));
    }
}
