//! Main-thread browser application for CCM Network Lab.
//!
//! The worker remains deliberately small: this crate owns controls, graph
//! input generation, trace decoding, playback, JSON interchange, and Canvas
//! rendering.  Simulation transitions are provided only by `ccm-wasm`.

use js_sys::{Array, Int32Array, Object, Reflect, Uint32Array, Uint8Array};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{closure::Closure, prelude::wasm_bindgen, JsCast, JsValue};
use web_sys::{
    Blob, CanvasRenderingContext2d, Document, Element, Event, FileReader, HtmlAnchorElement,
    HtmlCanvasElement, HtmlElement, HtmlInputElement, HtmlSelectElement, MessageEvent, MouseEvent,
    ResizeObserver, Worker,
};

const COLORS: [&str; 3] = ["#39d3b4", "#ff7183", "#f6b44f"];
const MAX_IMPORT_EDGES: usize = 200_000;
const MAX_IMPORT_FRAMES: usize = 1_000_002;
const MAX_PORT_LABEL_EDGES: usize = 180;
const MAX_PORT_LABEL_DEGREE: usize = 12;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub family: String,
    #[serde(rename = "nodeCount")]
    pub node_count: usize,
    pub edges: Vec<u32>,
    #[serde(rename = "gridColumns")]
    pub grid_columns: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub algorithm: String,
    pub family: String,
    pub placement: String,
    #[serde(rename = "nodeCount")]
    pub node_count: usize,
    #[serde(rename = "agentCount")]
    pub agent_count: usize,
    pub seed: u32,
    #[serde(rename = "roundLimit")]
    pub round_limit: u64,
    #[serde(rename = "gridColumns")]
    pub grid_columns: usize,
    #[serde(rename = "traceMode")]
    pub trace_mode: String,
    #[serde(rename = "maxTraceRecords")]
    pub max_trace_records: usize,
    #[serde(rename = "sampleEvery")]
    pub sample_every: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub step: u64,
    pub positions: Vec<u32>,
    pub statuses: Vec<u8>,
    pub homes: Vec<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResultData {
    pub positions: Vec<u32>,
    pub statuses: Vec<u8>,
    pub homes: Vec<i32>,
    #[serde(rename = "terminationCode")]
    pub termination_code: u8,
    pub rounds: u64,
    pub moves: u64,
    pub probes: u64,
    #[serde(rename = "renderRecordCount")]
    pub render_record_count: u32,
    #[serde(rename = "renderTruncated")]
    pub render_truncated: bool,
    #[serde(rename = "renderBytes", default, skip_serializing)]
    pub render_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
struct ExportPayload<'a> {
    schema: &'static str,
    #[serde(rename = "exportedAt")]
    exported_at: String,
    config: &'a Config,
    graph: &'a Graph,
    starts: &'a [u32],
    result: ExportResult<'a>,
    frames: &'a [Frame],
}

#[derive(Clone, Debug, Serialize)]
struct ExportResult<'a> {
    positions: &'a [u32],
    statuses: &'a [u8],
    homes: &'a [i32],
    #[serde(rename = "terminationCode")]
    termination_code: u8,
    rounds: u64,
    moves: u64,
    probes: u64,
    #[serde(rename = "renderRecordCount")]
    render_record_count: u32,
    #[serde(rename = "renderTruncated")]
    render_truncated: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct ImportPayload {
    schema: String,
    config: Config,
    graph: Graph,
    #[serde(default)]
    starts: Vec<u32>,
    result: ResultData,
    frames: Vec<Frame>,
}

struct AppState {
    document: Document,
    canvas: HtmlCanvasElement,
    wrap: Element,
    worker: Option<Worker>,
    observer: Option<ResizeObserver>,
    busy: bool,
    graph: Option<Graph>,
    starts: Vec<u32>,
    config: Option<Config>,
    result: Option<ResultData>,
    frames: Vec<Frame>,
    frame: usize,
    timer: Option<i32>,
    layout: Vec<(f64, f64)>,
}

fn element<T: JsCast>(document: &Document, id: &str) -> T {
    document
        .get_element_by_id(id)
        .unwrap_or_else(|| panic!("missing UI element #{id}"))
        .dyn_into::<T>()
        .unwrap_or_else(|_| panic!("UI element #{id} has wrong type"))
}

fn value(document: &Document, id: &str) -> String {
    element::<HtmlInputElement>(document, id).value()
}

fn select_value(document: &Document, id: &str) -> String {
    element::<HtmlSelectElement>(document, id).value()
}

fn set_text(document: &Document, id: &str, text: &str) {
    element::<Element>(document, id).set_text_content(Some(text));
}

fn set_class(document: &Document, id: &str, class_name: &str) {
    let _ = element::<Element>(document, id).set_attribute("class", class_name);
}

fn say(document: &Document, message: &str, kind: &str) {
    set_text(document, "message", message);
    set_class(document, "message", &format!("message {kind}"));
}

fn number(document: &Document, id: &str, fallback: usize, min: usize, max: usize) -> usize {
    let input = element::<HtmlInputElement>(document, id);
    let parsed = input.value().parse::<u64>().ok();
    let valid = parsed.is_some_and(|v| v >= min as u64 && v <= max as u64);
    if !valid {
        input.set_value(&fallback.to_string());
        fallback
    } else {
        parsed.unwrap_or(fallback as u64) as usize
    }
}

fn seeded_shuffle(size: usize, seed: u32) -> Vec<u32> {
    let mut values: Vec<u32> = (0..size as u32).collect();
    let mut x = if seed == 0 { 1 } else { seed };
    let random = |x: &mut u32| -> f64 {
        *x ^= *x << 13;
        *x ^= *x >> 17;
        *x ^= *x << 5;
        f64::from(*x) / 4_294_967_296.0
    };
    for i in (1..size).rev() {
        let j = (random(&mut x) * (i + 1) as f64).floor() as usize;
        values.swap(i, j);
    }
    values
}

pub fn make_graph(family: &str, n: usize, grid_columns: usize) -> Graph {
    make_graph_with_seed(family, n, grid_columns, 42)
}

pub fn make_graph_with_seed(family: &str, n: usize, grid_columns: usize, seed: u32) -> Graph {
    if family == "random" {
        let mut edges = Vec::new();
        let mut seen = BTreeSet::new();
        {
            let mut add_unique = |a: usize, b: usize| {
                if a != b && a < n && b < n {
                    let edge = if a < b { (a, b) } else { (b, a) };
                    if seen.insert(edge) {
                        edges.extend([edge.0 as u32, edge.1 as u32]);
                    }
                }
            };
            let order = seeded_shuffle(n, seed);
            let mut tree_state = if seed == 0 { 1 } else { seed };
            let mut next_tree = || {
                tree_state ^= tree_state << 13;
                tree_state ^= tree_state >> 17;
                tree_state ^= tree_state << 5;
                tree_state
            };
            for i in 1..order.len() {
                let parent = (next_tree() as usize) % i;
                add_unique(order[i] as usize, order[parent] as usize);
            }
        }
        let possible = n.saturating_mul(n.saturating_sub(1)) / 2;
        let target_edges = (n.saturating_sub(1) + n.saturating_mul(2)).min(possible);
        let mut state = if seed == 0 { 1 } else { seed };
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        let mut attempts = 0usize;
        while seen.len() < target_edges && attempts < target_edges.saturating_mul(20).max(20) {
            attempts += 1;
            let a = (next() as usize) % n.max(1);
            let b = (next() as usize) % n.max(1);
            if a != b && a < n && b < n {
                let edge = if a < b { (a, b) } else { (b, a) };
                if seen.insert(edge) {
                    edges.extend([edge.0 as u32, edge.1 as u32]);
                }
            }
        }
        return Graph {
            family: family.to_owned(),
            node_count: n,
            edges,
            grid_columns,
        };
    }
    let mut edges = Vec::new();
    let mut add = |a: usize, b: usize| {
        if a != b && a < n && b < n {
            edges.extend([a as u32, b as u32]);
        }
    };
    if family == "path" || family == "cycle" {
        for i in 0..n.saturating_sub(1) {
            add(i, i + 1);
        }
    }
    if family == "cycle" && n > 2 {
        add(n - 1, 0);
    }
    if family == "star" {
        for i in 1..n {
            add(0, i);
        }
    }
    if family == "complete" {
        for i in 0..n {
            for j in i + 1..n {
                add(i, j);
            }
        }
    }
    if family == "tree" {
        for i in 1..n {
            add((i - 1) / 2, i);
        }
    }
    let columns = grid_columns.max(1).min(n.max(1));
    if family == "grid" {
        for i in 0..n {
            if i % columns != columns - 1 {
                add(i, i + 1);
            }
            if i + columns < n {
                add(i, i + columns);
            }
        }
    }
    Graph {
        family: family.to_owned(),
        node_count: n,
        edges,
        grid_columns,
    }
}

fn read_config(document: &Document) -> Config {
    let node_count = number(document, "nodes", 26, 1, 10_000);
    let agent_count = number(document, "agents", 5.min(node_count), 1, node_count);
    element::<HtmlInputElement>(document, "agents").set_value(&agent_count.to_string());
    Config {
        algorithm: select_value(document, "algorithm"),
        family: select_value(document, "family"),
        placement: select_value(document, "placement"),
        node_count,
        agent_count,
        seed: number(document, "seed", 42, 0, u32::MAX as usize) as u32,
        round_limit: number(document, "roundLimit", 500, 1, 1_000_000) as u64,
        grid_columns: number(document, "gridColumns", 5, 1, 10_000),
        trace_mode: select_value(document, "traceMode"),
        max_trace_records: number(document, "traceLimit", 600, 0, 1_000_000),
        sample_every: number(document, "sampleEvery", 4, 1, 1_000_000),
    }
}

fn initial_frame(starts: &[u32]) -> Frame {
    Frame {
        step: 0,
        positions: starts.to_vec(),
        statuses: vec![1; starts.len()],
        homes: vec![-1; starts.len()],
        label: None,
    }
}

fn final_frame(result: &ResultData) -> Frame {
    Frame {
        step: result.rounds,
        positions: result.positions.clone(),
        statuses: result.statuses.clone(),
        homes: result.homes.clone(),
        label: None,
    }
}

fn same_frame(a: &Frame, b: &Frame) -> bool {
    a.step == b.step && a.positions == b.positions && a.statuses == b.statuses && a.homes == b.homes
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(n)?;
        let out = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(out)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|x| x[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|x| u16::from_le_bytes([x[0], x[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
    }
    fn u64(&mut self) -> Option<u64> {
        let bytes = self.take(8)?;
        Some(u64::from_le_bytes(bytes.try_into().ok()?))
    }
    fn optional(&mut self, width: usize) -> Option<i32> {
        if self.u8()? == 0 {
            Some(-1)
        } else if width == 2 {
            Some(self.u16()? as i32)
        } else {
            Some(self.u32()? as i32)
        }
    }
}

fn trace_frames(bytes: &[u8], starts: &[u32], result: &ResultData) -> Vec<Frame> {
    let initial = initial_frame(starts);
    if bytes.len() < 8 {
        return vec![initial, final_frame(result)];
    }
    let mut reader = Reader::new(bytes);
    let (version, kind, _flags, _reserved, count) = match (
        reader.u8(),
        reader.u8(),
        reader.u8(),
        reader.u8(),
        reader.u32(),
    ) {
        (Some(a), Some(b), Some(c), Some(d), Some(e)) => (a, b, c, d, e),
        _ => return vec![initial],
    };
    if version != 1 {
        return vec![initial];
    }
    let mut frames = Vec::new();
    let mut current = initial.clone();
    for _ in 0..count {
        let record_type = match reader.u8() {
            Some(v) => v,
            None => break,
        };
        let step = match reader.u64() {
            Some(v) => v,
            None => break,
        };
        if kind == 1 && record_type == 1 {
            let label = match reader.u8() {
                Some(v) => v,
                None => break,
            };
            let count = match reader.u32() {
                Some(v) => v as usize,
                None => break,
            };
            let mut positions = Vec::with_capacity(count);
            let mut statuses = Vec::with_capacity(count);
            for _ in 0..count {
                positions.push(match reader.u32() {
                    Some(v) => v,
                    None => return vec![initial],
                });
                statuses.push(match reader.u8() {
                    Some(v) => v,
                    None => return vec![initial],
                });
            }
            frames.push(Frame {
                step,
                positions,
                statuses,
                homes: vec![-1; count],
                label: Some(label),
            });
            continue;
        }
        if kind != 2 {
            break;
        }
        match record_type {
            2 => {
                let event = match reader.u8() {
                    Some(v) => v,
                    None => break,
                };
                let mut next = current.clone();
                next.step = step;
                match event {
                    0 => {
                        let _ = reader.u8();
                    }
                    1 => {
                        let agent = reader.u32();
                        let _ = reader.u32();
                        let to = reader.u32();
                        let _ = reader.u16();
                        let _ = reader.optional(2);
                        if let (Some(a), Some(t)) = (agent, to) {
                            if (a as usize) < next.positions.len() {
                                next.positions[a as usize] = t;
                            }
                        }
                    }
                    2 => {
                        let count = reader.u32().unwrap_or(0);
                        let mut ids = Vec::with_capacity(count as usize);
                        for _ in 0..count {
                            if let Some(id) = reader.u32() {
                                ids.push(id);
                            } else {
                                break;
                            }
                        }
                        let _ = reader.u32();
                        let to = reader.u32();
                        let _ = reader.u16();
                        if let Some(t) = to {
                            for id in ids {
                                if (id as usize) < next.positions.len() {
                                    next.positions[id as usize] = t;
                                }
                            }
                        }
                    }
                    3 => {
                        let agent = reader.u32();
                        let node = reader.u32();
                        if let (Some(a), Some(n)) = (agent, node) {
                            if (a as usize) < next.positions.len() {
                                next.positions[a as usize] = n;
                                next.statuses[a as usize] = 0;
                            }
                        }
                    }
                    4 => {
                        let agent = reader.u32();
                        let _ = reader.u8();
                        let status = reader.u8();
                        if let (Some(a), Some(s)) = (agent, status) {
                            if (a as usize) < next.statuses.len() {
                                next.statuses[a as usize] = s;
                            }
                        }
                    }
                    5 => {
                        let _ = reader.take(16);
                        let _ = reader.optional(2);
                        let _ = reader.optional(2);
                    }
                    6 => {
                        let _ = reader.u32();
                        let _ = reader.u8();
                    }
                    _ => break,
                }
                current = next.clone();
                frames.push(next);
            }
            3 => {
                let count = match reader.u32() {
                    Some(v) => v as usize,
                    None => break,
                };
                let mut positions = Vec::with_capacity(count);
                let mut statuses = Vec::with_capacity(count);
                let mut homes = Vec::with_capacity(count);
                for _ in 0..count {
                    positions.push(match reader.u32() {
                        Some(v) => v,
                        None => break,
                    });
                    statuses.push(match reader.u8() {
                        Some(v) => v,
                        None => break,
                    });
                    homes.push(reader.optional(4).unwrap_or(-1));
                }
                current = Frame {
                    step,
                    positions,
                    statuses,
                    homes,
                    label: None,
                };
                frames.push(current.clone());
            }
            _ => break,
        }
    }
    let final_state = final_frame(result);
    if frames.last().map_or(true, |f| !same_frame(f, &final_state)) {
        frames.push(final_state);
    }
    let mut output = Vec::with_capacity(frames.len() + 1);
    output.push(initial);
    output.extend(frames);
    output
}

fn checked(document: &Document, id: &str) -> bool {
    element::<HtmlInputElement>(document, id).checked()
}

fn set_disabled(element: &HtmlElement, disabled: bool) {
    if disabled {
        let _ = element.set_attribute("disabled", "disabled");
    } else {
        let _ = element.remove_attribute("disabled");
    }
}

fn set_busy(state: &Rc<RefCell<AppState>>, busy: bool) {
    let mut app = state.borrow_mut();
    app.busy = busy;
    set_disabled(&element::<HtmlElement>(&app.document, "run"), busy);
    set_disabled(&element::<HtmlElement>(&app.document, "cancel"), !busy);
    set_disabled(&element::<HtmlElement>(&app.document, "import"), busy);
}

fn update_frame_label(app: &AppState) {
    set_text(
        &app.document,
        "frameLabel",
        &format!(
            "Frame {} / {}",
            app.frame,
            app.frames.len().saturating_sub(1)
        ),
    );
}

fn set_timeline(app: &AppState) {
    let timeline = element::<HtmlInputElement>(&app.document, "timeline");
    timeline.set_max(&app.frames.len().saturating_sub(1).to_string());
    timeline.set_value(&app.frame.to_string());
    timeline.set_disabled(app.frames.len() <= 1);
    for id in ["prev", "play", "next"] {
        set_disabled(
            &element::<HtmlElement>(&app.document, id),
            app.frames.len() <= 1,
        );
    }
    update_frame_label(app);
}

fn set_frame(state: &Rc<RefCell<AppState>>, index: isize) {
    let mut app = state.borrow_mut();
    if app.frames.is_empty() {
        return;
    }
    app.frame = index.clamp(0, app.frames.len() as isize - 1) as usize;
    let timeline = element::<HtmlInputElement>(&app.document, "timeline");
    timeline.set_value(&app.frame.to_string());
    update_frame_label(&app);
    drop(app);
    render(state);
}

fn layout_for(graph: &Graph, width: f64, height: f64) -> Vec<(f64, f64)> {
    let n = graph.node_count;
    let margin = 30.0_f64.max(width.min(height) * 0.08);
    let mut points = vec![(width / 2.0, height / 2.0); n];
    if graph.family == "path" {
        for (i, point) in points.iter_mut().enumerate() {
            point.0 = margin
                + (width - margin * 2.0)
                    * if n == 1 {
                        0.5
                    } else {
                        i as f64 / (n - 1) as f64
                    };
            point.1 = height / 2.0 + (i as f64 * 0.55).sin() * height * 0.06;
        }
    } else if graph.family == "grid" {
        let columns = graph.grid_columns.max(1);
        let rows = if n == 0 { 0 } else { 1 + (n - 1) / columns };
        for (i, point) in points.iter_mut().enumerate() {
            point.0 = margin
                + (width - margin * 2.0) * (i % columns) as f64
                    / columns.saturating_sub(1).max(1) as f64;
            point.1 = margin
                + (height - margin * 2.0) * (i / columns) as f64
                    / rows.saturating_sub(1).max(1) as f64;
        }
    } else if graph.family == "random" {
        let usable_width = (width - margin * 2.0).max(1.0);
        let usable_height = (height - margin * 2.0).max(1.0);
        for (i, point) in points.iter_mut().enumerate() {
            point.0 = margin + usable_width * layout_noise(i, 0x9e37_79b9);
            point.1 = margin + usable_height * layout_noise(i, 0x85eb_ca6b);
        }
    } else {
        let radius = 20.0_f64.max(width.min(height) / 2.0 - margin);
        for (i, point) in points.iter_mut().enumerate() {
            let angle =
                -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::TAU / n.max(1) as f64;
            point.0 = width / 2.0 + angle.cos() * radius;
            point.1 = height / 2.0 + angle.sin() * radius;
        }
    }
    if graph.family == "random" && n <= 600 {
        spring_layout(graph, width, height, points)
    } else {
        points
    }
}

fn layout_noise(index: usize, salt: u32) -> f64 {
    let mut value = (index as u32).wrapping_add(1).wrapping_mul(salt) ^ 0xa5a5_5a5a;
    value ^= value << 13;
    value ^= value >> 17;
    value ^= value << 5;
    f64::from(value) / 4_294_967_296.0
}

fn spring_layout(
    graph: &Graph,
    width: f64,
    height: f64,
    mut points: Vec<(f64, f64)>,
) -> Vec<(f64, f64)> {
    let n = graph.node_count;
    if n < 2 {
        return points;
    }
    let margin = 30.0_f64.max(width.min(height) * 0.08);
    let area = (width - margin * 2.0).max(1.0) * (height - margin * 2.0).max(1.0);
    let ideal = (area / n as f64).sqrt().clamp(12.0, 100.0);
    let mut temperature = width.min(height) * 0.12;
    for _ in 0..80 {
        let mut force = vec![(0.0, 0.0); n];
        for a in 0..n {
            for b in (a + 1)..n {
                let dx = points[a].0 - points[b].0;
                let dy = points[a].1 - points[b].1;
                let distance = (dx * dx + dy * dy).sqrt().max(1.0);
                let strength = (ideal * ideal / distance * 0.06).min(ideal * 2.0);
                let fx = dx / distance * strength;
                let fy = dy / distance * strength;
                force[a].0 += fx;
                force[a].1 += fy;
                force[b].0 -= fx;
                force[b].1 -= fy;
            }
        }
        for edge in graph.edges.chunks_exact(2) {
            let a = edge[0] as usize;
            let b = edge[1] as usize;
            let (Some(a_point), Some(b_point)) = (points.get(a), points.get(b)) else {
                continue;
            };
            let dx = b_point.0 - a_point.0;
            let dy = b_point.1 - a_point.1;
            let distance = (dx * dx + dy * dy).sqrt().max(1.0);
            let strength = distance * distance / ideal * 0.06;
            let fx = dx / distance * strength;
            let fy = dy / distance * strength;
            force[a].0 += fx;
            force[a].1 += fy;
            force[b].0 -= fx;
            force[b].1 -= fy;
        }
        for i in 0..n {
            force[i].0 += (width / 2.0 - points[i].0) * 0.025;
            force[i].1 += (height / 2.0 - points[i].1) * 0.025;
            let magnitude = force[i].0.hypot(force[i].1).max(1.0);
            let step = magnitude.min(temperature) / magnitude;
            points[i].0 += force[i].0 * step;
            points[i].1 += force[i].1 * step;
        }
        temperature *= 0.94;
    }
    fit_layout(points, width, height, margin)
}

fn fit_layout(
    mut points: Vec<(f64, f64)>,
    width: f64,
    height: f64,
    margin: f64,
) -> Vec<(f64, f64)> {
    let min_x = points
        .iter()
        .map(|point| point.0)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point.1)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    let scale = (((width - margin * 2.0).max(1.0) / span_x)
        .min((height - margin * 2.0).max(1.0) / span_y)
        * 0.94)
        .max(0.01);
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;
    for point in &mut points {
        point.0 = (width / 2.0 + (point.0 - center_x) * scale).clamp(margin, width - margin);
        point.1 = (height / 2.0 + (point.1 - center_y) * scale).clamp(margin, height - margin);
    }
    points
}

fn edge_port_labels(graph: &Graph) -> Vec<(usize, usize)> {
    let mut neighbors = vec![Vec::<usize>::new(); graph.node_count];
    for edge in graph.edges.chunks_exact(2) {
        let a = edge[0] as usize;
        let b = edge[1] as usize;
        if a < graph.node_count && b < graph.node_count {
            neighbors[a].push(b);
            neighbors[b].push(a);
        }
    }
    for list in &mut neighbors {
        list.sort_unstable();
    }
    graph
        .edges
        .chunks_exact(2)
        .map(|edge| {
            let a = edge[0] as usize;
            let b = edge[1] as usize;
            let a_port = neighbors
                .get(a)
                .and_then(|list| list.binary_search(&b).ok())
                .unwrap_or(0);
            let b_port = neighbors
                .get(b)
                .and_then(|list| list.binary_search(&a).ok())
                .unwrap_or(0);
            (a_port, b_port)
        })
        .collect()
}

fn port_labels_are_legible(graph: &Graph) -> bool {
    let edge_count = graph.edges.len() / 2;
    if edge_count > MAX_PORT_LABEL_EDGES {
        return false;
    }
    let mut degrees = vec![0_usize; graph.node_count];
    for edge in graph.edges.chunks_exact(2) {
        let (a, b) = (edge[0] as usize, edge[1] as usize);
        if let Some(degree) = degrees.get_mut(a) {
            *degree += 1;
        }
        if let Some(degree) = degrees.get_mut(b) {
            *degree += 1;
        }
    }
    degrees.into_iter().max().unwrap_or(0) <= MAX_PORT_LABEL_DEGREE
}

fn draw_port_badge(
    context: &CanvasRenderingContext2d,
    label: &str,
    x: f64,
    y: f64,
    foreground: &str,
    background: &str,
) {
    let width = 7.0 + label.len() as f64 * 6.0;
    context.set_fill_style_str(background);
    context.fill_rect(x - width / 2.0, y - 6.5, width, 13.0);
    context.set_fill_style_str(foreground);
    let _ = context.fill_text(label, x, y + 0.5);
}

fn theme_colors(document: &Document) -> (String, String, String) {
    let window = web_sys::window().expect("window");
    let root = document.document_element().expect("root");
    let styles = window.get_computed_style(&root).ok().flatten();
    let border = styles
        .as_ref()
        .and_then(|s| s.get_property_value("--border").ok())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "#52617a".into());
    let fill = styles
        .as_ref()
        .and_then(|s| s.get_property_value("--panel-2").ok())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "#1d2940".into());
    let text = styles
        .and_then(|s| s.get_property_value("--text").ok())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "#e8eef9".into());
    (border, fill, text)
}

fn render(state: &Rc<RefCell<AppState>>) {
    let app = state.borrow();
    let context = match app
        .canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|v| v.dyn_into::<CanvasRenderingContext2d>().ok())
    {
        Some(c) => c,
        None => return,
    };
    let rect = app.canvas.get_bounding_client_rect();
    let width = rect.width();
    let height = rect.height();
    let dpr = web_sys::window().map_or(1.0, |window| window.device_pixel_ratio());
    let _ = context.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    context.clear_rect(0.0, 0.0, width, height);
    let (graph, frame) = match (app.graph.as_ref(), app.frames.get(app.frame)) {
        (Some(g), Some(f)) => (g, f),
        _ => return,
    };
    let points = if app.layout.len() == graph.node_count {
        app.layout.clone()
    } else {
        layout_for(graph, width, height)
    };
    let (border, empty_fill, text_color) = theme_colors(&app.document);
    let show_edges = checked(&app.document, "showEdges");
    let ports_requested = checked(&app.document, "showPorts");
    let ports_legible = port_labels_are_legible(graph);
    let show_ports = show_edges && ports_requested && ports_legible;
    set_text(
        &app.document,
        "legendTip",
        if show_edges && ports_requested && !ports_legible {
            "Port badges hidden at this density; hover a node for its full port table"
        } else {
            "Hover a node for details"
        },
    );
    let port_labels = show_ports.then(|| edge_port_labels(graph));
    let show_agents = checked(&app.document, "showAgents");
    let draw_individuals = show_agents && graph.node_count <= 2500;
    if show_edges {
        context.set_stroke_style_str(&border);
        context.set_line_width(if graph.node_count > 800 { 0.55 } else { 1.0 });
        context.begin_path();
        for edge in graph.edges.chunks_exact(2) {
            if let (Some(a), Some(b)) = (points.get(edge[0] as usize), points.get(edge[1] as usize))
            {
                context.move_to(a.0, a.1);
                context.line_to(b.0, b.1);
            }
        }
        context.stroke();
    }
    if let Some(port_labels) = port_labels.as_ref() {
        context.set_font("10px ui-monospace, SFMono-Regular, Menlo, monospace");
        context.set_text_align("center");
        context.set_text_baseline("middle");
        context.set_fill_style_str(&border);
        for (index, edge) in graph.edges.chunks_exact(2).enumerate() {
            let (Some(a), Some(b), Some((a_port, b_port))) = (
                points.get(edge[0] as usize),
                points.get(edge[1] as usize),
                port_labels.get(index),
            ) else {
                continue;
            };
            let dx = b.0 - a.0;
            let dy = b.1 - a.1;
            let distance = dx.hypot(dy).max(1.0);
            let ux = dx / distance;
            let uy = dy / distance;
            let nx = -uy;
            let ny = ux;
            let a_label = format!("p{a_port}");
            let b_label = format!("p{b_port}");
            let a_distance = 18.0 + (a_port % 4) as f64 * 10.0;
            let b_distance = 18.0 + (b_port % 4) as f64 * 10.0;
            let a_side = if (a_port / 4) % 2 == 0 { 1.0 } else { -1.0 };
            let b_side = if (b_port / 4) % 2 == 0 { 1.0 } else { -1.0 };
            draw_port_badge(
                &context,
                &a_label,
                a.0 + ux * a_distance + nx * 6.0 * a_side,
                a.1 + uy * a_distance + ny * 6.0 * a_side,
                &text_color,
                &empty_fill,
            );
            draw_port_badge(
                &context,
                &b_label,
                b.0 - ux * b_distance - nx * 6.0 * b_side,
                b.1 - uy * b_distance - ny * 6.0 * b_side,
                &text_color,
                &empty_fill,
            );
        }
    }
    let mut by_node: Vec<Vec<usize>> = vec![Vec::new(); points.len()];
    if show_agents {
        for (agent, node) in frame.positions.iter().enumerate() {
            let status = frame.statuses.get(agent).copied().unwrap_or(1);
            let visible = match status {
                0 => checked(&app.document, "showSettled"),
                2 => checked(&app.document, "showScout"),
                _ => checked(&app.document, "showUnsettled"),
            };
            if visible {
                if let Some(bucket) = by_node.get_mut(*node as usize) {
                    bucket.push(agent);
                }
            }
        }
    }
    if graph.node_count > 2500 {
        for (node, point) in points.iter().enumerate() {
            let agents = &by_node[node];
            context.set_fill_style_str(if let Some(agent) = agents.first() {
                COLORS[frame.statuses.get(*agent).copied().unwrap_or(0) as usize % 3]
            } else {
                &empty_fill
            });
            context.fill_rect(point.0 - 1.0, point.1 - 1.0, 2.0, 2.0);
        }
        return;
    }
    let radius = (2.5_f64).max((9.0_f64).min(12.0 - graph.node_count as f64 / 1800.0));
    for (node, point) in points.iter().enumerate() {
        let agents = &by_node[node];
        let color = agents
            .first()
            .map(|a| COLORS[frame.statuses.get(*a).copied().unwrap_or(0) as usize % 3])
            .unwrap_or(&border);
        context.set_fill_style_str(if agents.is_empty() {
            &empty_fill
        } else {
            "#314875"
        });
        context.set_stroke_style_str(color);
        context.set_line_width(if agents.is_empty() { 1.0 } else { 2.0 });
        context.begin_path();
        let _ = context.arc(
            point.0,
            point.1,
            radius + if agents.is_empty() { 0.0 } else { 1.0 },
            0.0,
            std::f64::consts::TAU,
        );
        context.fill();
        context.stroke();
        if draw_individuals {
            for (index, agent) in agents.iter().enumerate() {
                let angle = index as f64 * std::f64::consts::TAU / agents.len().max(1) as f64;
                let distance = radius + 5.0 + agents.len().min(10) as f64;
                context.set_fill_style_str(
                    COLORS[frame.statuses.get(*agent).copied().unwrap_or(0) as usize % 3],
                );
                context.begin_path();
                let _ = context.arc(
                    point.0 + angle.cos() * distance,
                    point.1 + angle.sin() * distance,
                    (2.0_f64).max(radius * 0.58),
                    0.0,
                    std::f64::consts::TAU,
                );
                context.fill();
            }
        }
    }
}

fn resize(state: &Rc<RefCell<AppState>>) {
    let mut app = state.borrow_mut();
    let rect = app.wrap.get_bounding_client_rect();
    let dpr = web_sys::window()
        .and_then(|w| w.device_pixel_ratio().into())
        .unwrap_or(1.0);
    app.canvas.set_width((rect.width() * dpr).max(1.0) as u32);
    app.canvas.set_height((rect.height() * dpr).max(1.0) as u32);
    app.layout = app
        .graph
        .as_ref()
        .map(|g| layout_for(g, rect.width(), rect.height()))
        .unwrap_or_default();
    drop(app);
    render(state);
}

fn reflected_value(data: &JsValue, name: &str) -> Result<JsValue, String> {
    Reflect::get(data, &JsValue::from_str(name))
        .map_err(|_| format!("worker result is missing {name}"))
}

fn reflected_u64(data: &JsValue, name: &str) -> Result<u64, String> {
    let value = reflected_value(data, name)?
        .as_f64()
        .ok_or_else(|| format!("worker result {name} is not numeric"))?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > u64::MAX as f64 {
        return Err(format!(
            "worker result {name} is outside the supported range"
        ));
    }
    Ok(value as u64)
}

fn result_from_message(data: &JsValue) -> Result<ResultData, String> {
    let positions = reflected_value(data, "positions")?;
    let statuses = reflected_value(data, "statuses")?;
    let homes = reflected_value(data, "homes")?;
    let render_bytes = reflected_value(data, "renderBytes")?;
    if !positions.is_instance_of::<Uint32Array>()
        || !statuses.is_instance_of::<Uint8Array>()
        || !homes.is_instance_of::<Int32Array>()
        || !render_bytes.is_instance_of::<Uint8Array>()
    {
        return Err("worker result contains invalid typed arrays".to_owned());
    }
    Ok(ResultData {
        positions: Uint32Array::new(&positions).to_vec(),
        statuses: Uint8Array::new(&statuses).to_vec(),
        homes: Int32Array::new(&homes).to_vec(),
        termination_code: u8::try_from(reflected_u64(data, "terminationCode")?)
            .map_err(|_| "worker termination code is invalid".to_owned())?,
        rounds: reflected_u64(data, "rounds")?,
        moves: reflected_u64(data, "moves")?,
        probes: reflected_u64(data, "probes")?,
        render_record_count: u32::try_from(reflected_u64(data, "renderRecordCount")?)
            .map_err(|_| "worker render record count is invalid".to_owned())?,
        render_truncated: reflected_value(data, "renderTruncated")?
            .as_bool()
            .ok_or_else(|| "worker render truncation flag is invalid".to_owned())?,
        render_bytes: Uint8Array::new(&render_bytes).to_vec(),
    })
}

fn termination(code: u8) -> &'static str {
    [
        "completed",
        "round limit reached",
        "cancelled",
        "invalid / invariant failure",
    ]
    .get(code as usize)
    .copied()
    .unwrap_or("unknown termination")
}

fn accept_result(state: &Rc<RefCell<AppState>>, result: ResultData) {
    let mut app = state.borrow_mut();
    let starts = app.starts.clone();
    app.frames = trace_frames(&result.render_bytes, &starts, &result);
    app.frame = 0;
    app.result = Some(result.clone());
    let config = app.config.clone();
    set_text(&app.document, "rounds", &result.rounds.to_string());
    set_text(&app.document, "moves", &result.moves.to_string());
    set_text(&app.document, "probes", &result.probes.to_string());
    set_text(
        &app.document,
        "recordCount",
        &format!(
            "{}{}",
            result.render_record_count,
            if result.render_truncated { "+" } else { "" }
        ),
    );
    let _ = element::<Element>(&app.document, "emptyState")
        .class_list()
        .add_1("hidden");
    if let Some(c) = config {
        set_text(
            &app.document,
            "runTitle",
            &format!(
                "{} · {} · {}",
                if c.algorithm == "drop" {
                    "Drop and Freeze"
                } else {
                    "Help by Scouts"
                },
                c.family,
                termination(result.termination_code)
            ),
        );
    }
    set_timeline(&app);
    set_disabled(&element::<HtmlElement>(&app.document, "export"), false);
    say(
        &app.document,
        &format!(
            "Completed with {} playback frames{}.",
            app.frames.len(),
            if result.render_truncated {
                " (trace bounded)"
            } else {
                ""
            }
        ),
        "",
    );
    drop(app);
    resize(state);
}

fn post_run(
    worker: &Worker,
    config: &Config,
    graph: &Graph,
    starts: &[u32],
) -> Result<(), JsValue> {
    let message = Object::new();
    Reflect::set(&message, &"type".into(), &"run".into())?;
    Reflect::set(
        &message,
        &"algorithm".into(),
        &config.algorithm.clone().into(),
    )?;
    Reflect::set(
        &message,
        &"traceMode".into(),
        &config.trace_mode.clone().into(),
    )?;
    Reflect::set(
        &message,
        &"nodeCount".into(),
        &(graph.node_count as u32).into(),
    )?;
    Reflect::set(
        &message,
        &"edges".into(),
        &Uint32Array::from(graph.edges.as_slice()),
    )?;
    Reflect::set(&message, &"starts".into(), &Uint32Array::from(starts))?;
    Reflect::set(
        &message,
        &"roundLimit".into(),
        &(config.round_limit as f64).into(),
    )?;
    Reflect::set(
        &message,
        &"maxTraceRecords".into(),
        &(config.max_trace_records as u32).into(),
    )?;
    Reflect::set(
        &message,
        &"sampleEvery".into(),
        &(config.sample_every as u32).into(),
    )?;
    worker.post_message(&message)
}

fn create_worker(state: &Rc<RefCell<AppState>>) {
    let old = state.borrow_mut().worker.take();
    if let Some(worker) = old {
        worker.terminate();
    }
    let worker_url = format!("./worker.js?session={:.0}", js_sys::Date::now());
    let worker = match Worker::new(&worker_url) {
        Ok(w) => w,
        Err(_) => {
            let app = state.borrow();
            set_text(&app.document, "engineState", "WASM worker error");
            say(
                &app.document,
                "Worker failed. Build the browser package with ./scripts/build-wasm.sh.",
                "error",
            );
            return;
        }
    };
    let ready_state = Rc::clone(state);
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let data = event.data();
        let typ = Reflect::get(&data, &"type".into())
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let app = ready_state.borrow();
        if typ == "ready" {
            set_text(&app.document, "engineState", "WASM ready");
            set_class(&app.document, "engineState", "engine-state ready");
        } else if typ == "progress" {
            let message = Reflect::get(&data, &"message".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "Running…".into());
            say(&app.document, &message, "busy");
        } else if typ == "error" {
            let message = Reflect::get(&data, &"message".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "WASM worker error".into());
            set_class(&app.document, "engineState", "engine-state error");
            say(&app.document, &message, "error");
            drop(app);
            set_busy(&ready_state, false);
        } else if typ == "result" {
            let result = Reflect::get(&data, &"result".into())
                .map_err(|_| "worker response did not contain a result".to_owned())
                .and_then(|value| result_from_message(&value));
            drop(app);
            match result {
                Ok(result) => {
                    accept_result(&ready_state, result);
                    set_busy(&ready_state, false);
                }
                Err(message) => {
                    let app = ready_state.borrow();
                    say(&app.document, &message, "error");
                    drop(app);
                    set_busy(&ready_state, false);
                }
            }
        }
    });
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
    let error_state = Rc::clone(state);
    let onerror = Closure::<dyn FnMut(Event)>::new(move |_| {
        let app = error_state.borrow();
        set_class(&app.document, "engineState", "engine-state error");
        say(
            &app.document,
            "Worker failed. Build the browser package with ./scripts/build-wasm.sh.",
            "error",
        );
        drop(app);
        set_busy(&error_state, false);
    });
    worker.set_onerror(Some(onerror.as_ref().unchecked_ref()));
    onerror.forget();
    state.borrow_mut().worker = Some(worker);
}

fn run(state: &Rc<RefCell<AppState>>) {
    let document = state.borrow().document.clone();
    let mut config = read_config(&document);
    if config.agent_count > config.node_count {
        say(&document, "Agents cannot exceed nodes.", "error");
        return;
    }
    if config.family == "complete" && config.node_count > 500 {
        say(&document, "Complete graphs are capped at 500 nodes in the visualizer; use the native CLI for larger dense cases.", "error");
        return;
    }
    if config.node_count > 1000 && config.trace_mode == "full" {
        config.trace_mode = "bounded".into();
        config.max_trace_records = config.max_trace_records.min(256);
        config.sample_every = config.sample_every.max(10);
        element::<HtmlSelectElement>(&document, "traceMode").set_value("bounded");
        say(
            &document,
            "Large visualization: switched to a bounded sampled trace.",
            "busy",
        );
    }
    let graph = make_graph_with_seed(
        &config.family,
        config.node_count,
        config.grid_columns,
        config.seed,
    );
    let starts = if config.placement == "rooted" {
        vec![0; config.agent_count]
    } else {
        seeded_shuffle(config.node_count, config.seed)[..config.agent_count].to_vec()
    };
    {
        let mut app = state.borrow_mut();
        app.config = Some(config.clone());
        app.graph = Some(graph.clone());
        app.starts = starts.clone();
        app.result = None;
        app.frames.clear();
        app.layout.clear();
        set_disabled(&element::<HtmlElement>(&app.document, "export"), true);
    }
    set_busy(state, true);
    set_text(
        &document,
        "runTitle",
        &format!(
            "{} · {}",
            if config.algorithm == "drop" {
                "Drop and Freeze"
            } else {
                "Help by Scouts"
            },
            config.family
        ),
    );
    say(&document, "Preparing graph and starting Rust…", "busy");
    create_worker(state);
    let worker = state.borrow().worker.clone();
    if let Some(worker) = worker {
        if post_run(&worker, &config, &graph, &starts).is_err() {
            say(&document, "Unable to start the Rust worker.", "error");
            set_busy(state, false);
        }
    }
}

fn cancel(state: &Rc<RefCell<AppState>>) {
    let mut app = state.borrow_mut();
    if !app.busy {
        return;
    }
    if let Some(worker) = app.worker.take() {
        worker.terminate();
    }
    app.busy = false;
    drop(app);
    set_busy(state, false);
    let app = state.borrow();
    set_text(&app.document, "engineState", "Worker stopped");
    say(&app.document, "Cancelled. Ready for another run.", "");
}

fn toggle_play(state: &Rc<RefCell<AppState>>) {
    let mut app = state.borrow_mut();
    if let Some(timer) = app.timer.take() {
        if let Some(window) = web_sys::window() {
            window.clear_interval_with_handle(timer);
        }
        set_text(&app.document, "play", "▶");
        return;
    }
    if app.frames.len() <= 1 {
        return;
    }
    if app.frame + 1 >= app.frames.len() {
        app.frame = 0;
    }
    set_text(&app.document, "play", "Ⅱ");
    let interval = value(&app.document, "speed").parse::<i32>().unwrap_or(240);
    let weak = Rc::clone(state);
    let callback = Closure::<dyn FnMut()>::new(move || {
        let done = {
            let app = weak.borrow();
            app.frame + 1 >= app.frames.len()
        };
        if done {
            let mut app = weak.borrow_mut();
            if let Some(timer) = app.timer.take() {
                web_sys::window()
                    .expect("window")
                    .clear_interval_with_handle(timer);
            }
            set_text(&app.document, "play", "▶");
        } else {
            let next = weak.borrow().frame as isize + 1;
            set_frame(&weak, next);
        }
    });
    let timer = web_sys::window()
        .expect("window")
        .set_interval_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            interval,
        )
        .unwrap_or(-1);
    callback.forget();
    app.timer = Some(timer);
}

fn export_json(state: &Rc<RefCell<AppState>>) {
    let app = state.borrow();
    let (config, graph, result) =
        match (app.config.as_ref(), app.graph.as_ref(), app.result.as_ref()) {
            (Some(c), Some(g), Some(r)) => (c, g, r),
            _ => return,
        };
    let payload = ExportPayload {
        schema: "ccm-browser-v1",
        exported_at: js_sys::Date::new_0().to_iso_string().into(),
        config,
        graph,
        starts: &app.starts,
        result: ExportResult {
            positions: &result.positions,
            statuses: &result.statuses,
            homes: &result.homes,
            termination_code: result.termination_code,
            rounds: result.rounds,
            moves: result.moves,
            probes: result.probes,
            render_record_count: result.render_record_count,
            render_truncated: result.render_truncated,
        },
        frames: &app.frames,
    };
    let text = match serde_json::to_string_pretty(&payload) {
        Ok(v) => v,
        Err(_) => return,
    };
    let parts = Array::new();
    parts.push(&JsValue::from_str(&text));
    let blob = match Blob::new_with_str_sequence(&parts) {
        Ok(v) => v,
        Err(_) => return,
    };
    let url = match web_sys::Url::create_object_url_with_blob(&blob) {
        Ok(v) => v,
        Err(_) => return,
    };
    let anchor: HtmlAnchorElement = app
        .document
        .create_element("a")
        .expect("anchor")
        .dyn_into()
        .expect("anchor element");
    let _ = anchor.set_attribute("href", &url);
    let _ = anchor.set_attribute(
        "download",
        &format!("ccm-{}-{}.json", config.algorithm, config.family),
    );
    if let Some(body) = app.document.body() {
        let _ = body.append_child(&anchor);
    }
    anchor.click();
    anchor.remove();
    let revoke = Closure::<dyn FnMut()>::new(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    let _ = web_sys::window().and_then(|window| {
        window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                revoke.as_ref().unchecked_ref(),
                1_000,
            )
            .ok()
    });
    revoke.forget();
}

fn import_file(state: &Rc<RefCell<AppState>>, event: Event) {
    let input: HtmlInputElement = event
        .target()
        .and_then(|v| v.dyn_into().ok())
        .unwrap_or_else(|| element(&state.borrow().document, "import"));
    let file = match input.files().and_then(|files| files.get(0)) {
        Some(file) => file,
        None => return,
    };
    let reader = match FileReader::new() {
        Ok(v) => v,
        Err(_) => return,
    };
    let target = Rc::clone(state);
    let reader_copy = reader.clone();
    let onload = Closure::<dyn FnMut(Event)>::new(move |_| {
        let text = reader_copy.result().ok().and_then(|v| v.as_string());
        let Some(text) = text else { return };
        match serde_json::from_str::<ImportPayload>(&text) {
            Ok(data) if validate_import(&data).is_ok() => {
                let mut app = target.borrow_mut();
                app.config = Some(data.config);
                app.graph = Some(data.graph);
                app.starts = data.starts;
                app.result = Some(data.result.clone());
                app.frames = data.frames;
                app.frame = 0;
                app.layout.clear();
                let _ = element::<Element>(&app.document, "emptyState")
                    .class_list()
                    .add_1("hidden");
                set_text(&app.document, "rounds", &data.result.rounds.to_string());
                set_text(&app.document, "moves", &data.result.moves.to_string());
                set_text(&app.document, "probes", &data.result.probes.to_string());
                set_text(
                    &app.document,
                    "recordCount",
                    &data.result.render_record_count.to_string(),
                );
                set_timeline(&app);
                set_disabled(&element::<HtmlElement>(&app.document, "export"), false);
                set_text(&app.document, "runTitle", "Imported execution");
                say(
                    &app.document,
                    &format!("Imported {} playback frames.", app.frames.len()),
                    "",
                );
                drop(app);
                resize(&target);
            }
            Ok(data) => {
                let message = validate_import(&data)
                    .err()
                    .unwrap_or_else(|| "unknown validation error".to_owned());
                let app = target.borrow();
                say(&app.document, &format!("Import failed: {message}"), "error");
            }
            Err(_) => {
                let app = target.borrow();
                say(
                    &app.document,
                    "Import failed: expected a ccm-browser-v1 JSON execution.",
                    "error",
                );
            }
        }
    });
    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
    let _ = reader.read_as_text(&file);
}

fn validate_import(data: &ImportPayload) -> Result<(), String> {
    let node_count = data.graph.node_count;
    if data.schema != "ccm-browser-v1" {
        return Err("unsupported schema".to_owned());
    }
    if !(1..=10_000).contains(&node_count) {
        return Err("node count must be between 1 and 10,000".to_owned());
    }
    if data.config.node_count != node_count {
        return Err("configuration and graph node counts differ".to_owned());
    }
    let agent_count = data.config.agent_count;
    if agent_count == 0 || agent_count > node_count {
        return Err("agent count must be between 1 and the node count".to_owned());
    }
    if data.graph.edges.len() % 2 != 0 {
        return Err("edges must contain source/destination pairs".to_owned());
    }
    if data.graph.edges.len() / 2 > MAX_IMPORT_EDGES {
        return Err(format!(
            "graph exceeds the {MAX_IMPORT_EDGES}-edge display limit"
        ));
    }
    let mut edges = BTreeSet::new();
    for edge in data.graph.edges.chunks_exact(2) {
        let (a, b) = (edge[0] as usize, edge[1] as usize);
        if a >= node_count || b >= node_count {
            return Err("an edge endpoint is outside the graph".to_owned());
        }
        if a == b {
            return Err("self-loops are not supported".to_owned());
        }
        let normalized = if a < b { (a, b) } else { (b, a) };
        if !edges.insert(normalized) {
            return Err("parallel edges are not supported".to_owned());
        }
    }
    if data.frames.is_empty() || data.frames.len() > MAX_IMPORT_FRAMES {
        return Err("playback frame count is outside the supported range".to_owned());
    }
    if data.starts.len() != agent_count {
        return Err("initial placement length does not match the agent count".to_owned());
    }
    validate_agent_vectors(
        &data.starts,
        &vec![1; agent_count],
        &vec![-1; agent_count],
        node_count,
        agent_count,
        "initial placement",
    )?;
    validate_agent_vectors(
        &data.result.positions,
        &data.result.statuses,
        &data.result.homes,
        node_count,
        agent_count,
        "final result",
    )?;
    for frame in &data.frames {
        validate_agent_vectors(
            &frame.positions,
            &frame.statuses,
            &frame.homes,
            node_count,
            agent_count,
            "playback frame",
        )?;
    }
    if data.result.termination_code > 3 {
        return Err("termination code is invalid".to_owned());
    }
    Ok(())
}

fn validate_agent_vectors(
    positions: &[u32],
    statuses: &[u8],
    homes: &[i32],
    node_count: usize,
    agent_count: usize,
    context: &str,
) -> Result<(), String> {
    if positions.len() != agent_count || statuses.len() != agent_count || homes.len() != agent_count
    {
        return Err(format!("{context} dimensions do not match the agent count"));
    }
    if positions
        .iter()
        .any(|position| *position as usize >= node_count)
    {
        return Err(format!("{context} contains a position outside the graph"));
    }
    if statuses.iter().any(|status| *status > 2) {
        return Err(format!("{context} contains an invalid agent status"));
    }
    if homes
        .iter()
        .any(|home| *home < -1 || (*home >= 0 && *home as usize >= node_count))
    {
        return Err(format!("{context} contains a home outside the graph"));
    }
    Ok(())
}

fn hook_click(state: &Rc<RefCell<AppState>>, id: &str, callback: fn(&Rc<RefCell<AppState>>)) {
    let element: Element = element(&state.borrow().document, id);
    let state = Rc::clone(state);
    let closure = Closure::<dyn FnMut(Event)>::new(move |_| callback(&state));
    let _ = element.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
    closure.forget();
}

/// The mobile drawer and its dismiss scrim are one piece of state, so both
/// always receive the same `open` class rather than drifting apart.
fn set_panel_open(state: &Rc<RefCell<AppState>>, open: bool) {
    let app = state.borrow();
    for id in ["controlPanel", "panelScrim"] {
        let _ = element::<Element>(&app.document, id)
            .class_list()
            .toggle_with_force("open", open);
    }
}

#[wasm_bindgen]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("window unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("document unavailable"))?;
    let canvas: HtmlCanvasElement = element(&document, "networkCanvas");
    let wrap: Element = element(&document, "canvasWrap");
    let state = Rc::new(RefCell::new(AppState {
        document: document.clone(),
        canvas,
        wrap,
        worker: None,
        observer: None,
        busy: false,
        graph: None,
        starts: Vec::new(),
        config: None,
        result: None,
        frames: Vec::new(),
        frame: 0,
        timer: None,
        layout: Vec::new(),
    }));
    hook_click(&state, "run", run);
    hook_click(&state, "cancel", cancel);
    hook_click(&state, "export", export_json);
    hook_click(&state, "prev", |s| {
        let next = s.borrow().frame as isize - 1;
        set_frame(s, next);
    });
    hook_click(&state, "next", |s| {
        let next = s.borrow().frame as isize + 1;
        set_frame(s, next);
    });
    hook_click(&state, "play", toggle_play);
    let timeline: HtmlInputElement = element(&document, "timeline");
    {
        let state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            let n = value(&state.borrow().document, "timeline")
                .parse::<isize>()
                .unwrap_or(0);
            set_frame(&state, n);
        });
        let _ =
            timeline.add_event_listener_with_callback("input", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    let speed: HtmlInputElement = element(&document, "speed");
    {
        let state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            let app = state.borrow();
            set_text(
                &app.document,
                "speedValue",
                &format!("{} ms", value(&app.document, "speed")),
            );
        });
        let _ = speed.add_event_listener_with_callback("input", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    for id in [
        "showEdges",
        "showPorts",
        "showAgents",
        "showSettled",
        "showUnsettled",
        "showScout",
    ] {
        let checkbox: HtmlInputElement = element(&document, id);
        let state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| render(&state));
        let _ =
            checkbox.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    let import: HtmlInputElement = element(&document, "import");
    {
        let state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut(Event)>::new(move |event| import_file(&state, event));
        let _ = import.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    let family: HtmlSelectElement = element(&document, "family");
    {
        let document = document.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            let hidden = select_value(&document, "family") != "grid";
            let _ = element::<Element>(&document, "gridColumnsWrap")
                .class_list()
                .toggle_with_force("hidden", hidden);
        });
        let _ = family.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    let trace: HtmlSelectElement = element(&document, "traceMode");
    {
        let document = document.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            let mode = select_value(&document, "traceMode");
            let _ = element::<Element>(&document, "traceLimitWrap")
                .class_list()
                .toggle_with_force("hidden", mode == "off");
            let _ = element::<Element>(&document, "sampleEveryWrap")
                .class_list()
                .toggle_with_force("hidden", mode != "bounded");
        });
        let _ = trace.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    hook_click(&state, "themeToggle", |s| {
        let app = s.borrow();
        let root = app.document.document_element().expect("root");
        let dark = root
            .get_attribute("data-theme")
            .unwrap_or_else(|| "dark".into())
            == "dark";
        let theme = if dark { "light" } else { "dark" };
        let _ = root.set_attribute("data-theme", theme);
        if let Ok(Some(storage)) = web_sys::window().expect("window").local_storage() {
            let _ = storage.set_item("ccm-theme", theme);
        }
        drop(app);
        render(s);
    });
    hook_click(&state, "panelToggle", |s| set_panel_open(s, true));
    hook_click(&state, "closePanel", |s| set_panel_open(s, false));
    hook_click(&state, "panelScrim", |s| set_panel_open(s, false));
    let canvas: HtmlCanvasElement = element(&document, "networkCanvas");
    {
        let state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut(MouseEvent)>::new(move |event| tooltip(&state, event));
        let _ =
            canvas.add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    {
        let state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut(Event)>::new(move |_| {
            let app = state.borrow();
            let _ = element::<Element>(&app.document, "tooltip").set_attribute("hidden", "");
        });
        let _ =
            canvas.add_event_listener_with_callback("mouseleave", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    let observer_state = Rc::clone(&state);
    let observer_callback = Closure::<dyn FnMut(Array, ResizeObserver)>::new(move |_, _| {
        resize(&observer_state);
    });
    let observer = ResizeObserver::new(observer_callback.as_ref().unchecked_ref())?;
    observer.observe(&element::<Element>(&document, "canvasWrap"));
    observer_callback.forget();
    state.borrow_mut().observer = Some(observer);
    let state_resize = Rc::clone(&state);
    let closure = Closure::<dyn FnMut(Event)>::new(move |_| resize(&state_resize));
    window.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())?;
    closure.forget();
    resize(&state);
    if let Ok(Some(storage)) = window.local_storage() {
        if let Ok(Some(theme)) = storage.get_item("ccm-theme") {
            let _ = document
                .document_element()
                .expect("root")
                .set_attribute("data-theme", &theme);
        }
    }
    create_worker(&state);
    Ok(())
}

fn tooltip(state: &Rc<RefCell<AppState>>, event: MouseEvent) {
    let app = state.borrow();
    let graph = match app.graph.as_ref() {
        Some(v) => v,
        None => return,
    };
    let rect = app.canvas.get_bounding_client_rect();
    let x = f64::from(event.client_x()) - rect.left();
    let y = f64::from(event.client_y()) - rect.top();
    let points = if app.layout.len() == graph.node_count {
        &app.layout
    } else {
        return;
    };
    let nearest = points
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let d = (p.0 - x).hypot(p.1 - y);
            (d < 14.0).then_some((d, i))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|v| v.1);
    let tooltip: Element = element(&app.document, "tooltip");
    let Some(node) = nearest else {
        let _ = tooltip.set_attribute("hidden", "");
        return;
    };
    let frame = match app.frames.get(app.frame) {
        Some(v) => v,
        None => return,
    };
    let agents: Vec<String> = frame
        .positions
        .iter()
        .enumerate()
        .filter(|(_, position)| **position as usize == node)
        .map(|(i, position)| {
            let status = ["settled", "unsettled", "waiting"]
                [frame.statuses.get(i).copied().unwrap_or(1) as usize % 3];
            let home = frame
                .homes
                .get(i)
                .filter(|home| **home >= 0)
                .map_or_else(String::new, |home| format!(" · home {}", home));
            format!("Agent {}: node {} · {}{}", i, position, status, home)
        })
        .collect();
    let degree = graph.edges.iter().filter(|n| **n as usize == node).count();
    let mut port_neighbors = Vec::new();
    for edge in graph.edges.chunks_exact(2) {
        let a = edge[0] as usize;
        let b = edge[1] as usize;
        if a == node {
            port_neighbors.push(b);
        } else if b == node {
            port_neighbors.push(a);
        }
    }
    port_neighbors.sort_unstable();
    let ports = if port_neighbors.is_empty() {
        "No ports".to_owned()
    } else {
        format!(
            "Ports: {}",
            port_neighbors
                .iter()
                .enumerate()
                .map(|(port, neighbor)| format!("p{port}→{neighbor}"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    tooltip.set_inner_html(&format!(
        "<b>Node {node}</b><br>Degree {degree}<br>{ports}<br>{}",
        if agents.is_empty() {
            "No agents at this node".into()
        } else {
            format!("<b>Agents at node</b><br>{}", agents.join("<br>"))
        }
    ));
    let _ = tooltip.remove_attribute("hidden");
    let _ = tooltip.set_attribute(
        "style",
        &format!(
            "left:{}px;top:{}px",
            (x + 16.0).min(rect.width() - 245.0).max(8.0),
            (y - 12.0).max(8.0)
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_families_are_deterministic() {
        assert_eq!(make_graph("path", 4, 2).edges, vec![0, 1, 1, 2, 2, 3]);
        assert_eq!(make_graph("cycle", 3, 2).edges.len(), 6);
        assert_eq!(make_graph("star", 4, 2).edges, vec![0, 1, 0, 2, 0, 3]);
        let random = make_graph_with_seed("random", 20, 2, 7);
        assert_eq!(random.edges, make_graph_with_seed("random", 20, 2, 7).edges);
        assert_eq!(random.edges.len(), 2 * (19 + 40));
        let path = make_graph("path", 3, 2);
        assert_eq!(edge_port_labels(&path), vec![(0, 0), (1, 0)]);
        assert!(port_labels_are_legible(&path));
        assert!(!port_labels_are_legible(&make_graph("complete", 20, 2)));
        let spring = layout_for(&random, 800.0, 600.0);
        assert_eq!(spring, layout_for(&random, 800.0, 600.0));
        assert!(spring
            .iter()
            .all(|(x, y)| *x >= 30.0 && *x <= 770.0 && *y >= 30.0 && *y <= 570.0));
        let boundary_count = spring
            .iter()
            .filter(|(x, y)| *x <= 31.0 || *x >= 769.0 || *y <= 31.0 || *y >= 569.0)
            .count();
        assert!(boundary_count <= 4);
    }
    #[test]
    fn shuffle_is_seeded_and_distinct() {
        let a = seeded_shuffle(20, 42);
        assert_eq!(a, seeded_shuffle(20, 42));
        let mut sorted = a;
        sorted.sort_unstable();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
    }
    #[test]
    fn malformed_trace_does_not_panic() {
        let result = ResultData {
            positions: vec![0],
            statuses: vec![1],
            homes: vec![-1],
            termination_code: 0,
            rounds: 1,
            moves: 0,
            probes: 0,
            render_record_count: 1,
            render_truncated: false,
            render_bytes: vec![1, 2, 0, 0, 1],
        };
        assert_eq!(trace_frames(&result.render_bytes, &[0], &result).len(), 2);
    }

    #[test]
    fn imported_graphs_reject_invalid_endpoints_and_frame_dimensions() {
        let text = include_str!("../../../tests/fixtures/browser_import_v1.json");
        let mut payload: ImportPayload = serde_json::from_str(text).unwrap();
        assert!(validate_import(&payload).is_ok());
        payload.graph.edges[0] = payload.graph.node_count as u32;
        assert!(validate_import(&payload)
            .unwrap_err()
            .contains("endpoint is outside"));
        payload.graph.edges[0] = 0;
        payload.frames[0].statuses.pop();
        assert!(validate_import(&payload)
            .unwrap_err()
            .contains("dimensions"));
    }

    #[test]
    fn playback_keeps_records_that_do_not_change_positions() {
        let mut bytes = vec![1, 2, 0, 0];
        bytes.extend_from_slice(&2u32.to_le_bytes());
        for step in [1u64, 2] {
            bytes.push(2);
            bytes.extend_from_slice(&step.to_le_bytes());
            bytes.extend([0, 0]);
        }
        let result = ResultData {
            positions: vec![0],
            statuses: vec![1],
            homes: vec![-1],
            termination_code: 0,
            rounds: 2,
            moves: 0,
            probes: 0,
            render_record_count: 2,
            render_truncated: false,
            render_bytes: bytes,
        };
        let frames = trace_frames(&result.render_bytes, &[0], &result);
        assert_eq!(
            frames.iter().map(|frame| frame.step).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn browser_v1_fixture_imports() {
        let payload: ImportPayload = serde_json::from_str(include_str!(
            "../../../tests/fixtures/browser_import_v1.json"
        ))
        .expect("browser v1 fixture");
        assert_eq!(payload.schema, "ccm-browser-v1");
        assert_eq!(payload.graph.node_count, 2);
        assert_eq!(payload.frames.len(), 2);
        assert_eq!(payload.result.positions, vec![1]);
    }
}
