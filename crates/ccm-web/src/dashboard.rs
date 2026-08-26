//! The dashboard: a sweep over agent counts, plotted per graph class.
//!
//! Same controls as the simulator, but the workspace holds a chart and a table
//! instead of a canvas of the network. One algorithm is selected; for every
//! graph class chosen and every agent count in the range, one simulation runs
//! with tracing off and one cost counter is read out of it.
//!
//! The chart is a line chart because the x axis is an ordered quantity (agents)
//! and each graph class is an entity whose trend is the thing being compared.
//! Series colours are categorical — identity, not magnitude — assigned in fixed
//! slot order so adding or removing a class never repaints the others.

use crate::{
    checked, device_pixel_ratio, element, make_graph_full, number, say, select_value, set_class,
    set_disabled, set_text, Graph,
};
use js_sys::{Array, Reflect};
use serde::Serialize;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    Blob, CanvasRenderingContext2d, Document, Element, HtmlAnchorElement, HtmlCanvasElement,
    HtmlElement, MessageEvent, MouseEvent, ResizeObserver, Worker,
};

/// Graph classes offered as series, with the checkbox that selects each.
const FAMILIES: [(&str, &str, &str); 7] = [
    ("path", "Path", "famPath"),
    ("cycle", "Cycle", "famCycle"),
    ("star", "Star", "famStar"),
    ("complete", "Complete", "famComplete"),
    ("tree", "Balanced tree", "famTree"),
    ("grid", "Grid", "famGrid"),
    ("random", "Random connected", "famRandom"),
];

/// Categorical slots 1-7 of the reference palette, in fixed order.
///
/// Validated against this page's chart surfaces rather than assumed: all six
/// checks pass in both modes (worst adjacent CVD dE 9.1 light / 8.4 dark,
/// worst adjacent normal-vision dE 19.6 / 19.3). Three light-mode slots sit
/// below 3:1 on the light surface, which obliges the relief rule — hence the
/// legend and the table below the chart, both always present.
const SERIES_LIGHT: [&str; 7] = [
    "#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#4a3aa7",
];
const SERIES_DARK: [&str; 7] = [
    "#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9",
];

#[derive(Clone, Debug)]
struct SweepConfig {
    algorithm: String,
    families: Vec<usize>,
    nodes: usize,
    agent_from: usize,
    agent_to: usize,
    agent_step: usize,
    seed: u32,
    round_limit: u64,
    metric: String,
    grid_columns: usize,
    tree_branching: usize,
}

impl SweepConfig {
    /// The agent counts on the x axis, clamped to the node count: `k <= n`.
    fn agent_counts(&self) -> Vec<usize> {
        let step = self.agent_step.max(1);
        let last = self.agent_to.min(self.nodes);
        let mut counts = Vec::new();
        let mut at = self.agent_from.max(1);
        while at <= last {
            counts.push(at);
            at += step;
        }
        // Always include the endpoint, so the most interesting case (k = n) is
        // on the chart even when the step does not divide the range.
        if counts.last() != Some(&last) && last >= self.agent_from.max(1) {
            counts.push(last);
        }
        counts
    }

    fn metric_label(&self) -> &'static str {
        match self.metric.as_str() {
            "moves" => "agent moves",
            "probes" => "port probes",
            _ => "rounds",
        }
    }
}

/// One point: the cost of dispersing `agents` on one graph class.
#[derive(Clone, Copy, Debug, Serialize)]
struct Point {
    agents: usize,
    value: f64,
    completed: bool,
}

#[derive(Clone, Debug, Serialize)]
struct Series {
    family: String,
    label: String,
    slot: usize,
    points: Vec<Point>,
}

struct DashState {
    document: Document,
    canvas: HtmlCanvasElement,
    worker: Option<Worker>,
    observer: Option<ResizeObserver>,
    busy: bool,
    config: Option<SweepConfig>,
    series: Vec<Series>,
    hover: Option<usize>,
    size: (f64, f64),
}

/// Reads the sweep configuration out of the control panel.
fn read_config(document: &Document) -> SweepConfig {
    let nodes = number(document, "nodes", 40, 2, 2_000);
    let agent_from = number(document, "agentFrom", 2, 1, nodes);
    let agent_to = number(document, "agentTo", nodes, agent_from, nodes);
    SweepConfig {
        algorithm: select_value(document, "algorithm"),
        families: FAMILIES
            .iter()
            .enumerate()
            .filter(|(_, (_, _, id))| checked(document, id))
            .map(|(index, _)| index)
            .collect(),
        nodes,
        agent_from,
        agent_to,
        agent_step: number(document, "agentStep", 4, 1, 1_000),
        seed: u32::try_from(number(document, "seed", 42, 0, u32::MAX as usize)).unwrap_or(42),
        round_limit: number(document, "roundLimit", 20_000, 1, 5_000_000) as u64,
        metric: select_value(document, "metric"),
        grid_columns: number(document, "gridColumns", 6, 1, 1_000),
        tree_branching: number(document, "treeBranching", 2, 2, 16),
    }
}

fn set_busy(state: &Rc<RefCell<DashState>>, busy: bool) {
    let mut app = state.borrow_mut();
    app.busy = busy;
    let document = app.document.clone();
    drop(app);
    set_disabled(&element::<HtmlElement>(&document, "run"), busy);
    set_disabled(&element::<HtmlElement>(&document, "exportCsv"), busy);
}

/// Builds one job per (graph class, agent count) pair.
fn build_jobs(config: &SweepConfig) -> (Array, Vec<Series>) {
    let jobs = Array::new();
    let mut series = Vec::new();
    for &family_index in &config.families {
        let (family, label, _) = FAMILIES[family_index];
        let graph: Graph = make_graph_full(
            family,
            config.nodes,
            config.grid_columns,
            config.tree_branching,
            config.seed,
        );
        for agents in config.agent_counts() {
            let job = js_sys::Object::new();
            let _ = Reflect::set(&job, &"nodeCount".into(), &(graph.node_count as u32).into());
            let edges = js_sys::Uint32Array::from(&graph.edges[..]);
            let _ = Reflect::set(&job, &"edges".into(), &edges);
            // Rooted at node 0: P1Tree is defined for a rooted start, and using
            // the same placement for every algorithm keeps the comparison fair.
            let starts = js_sys::Uint32Array::new_with_length(agents as u32);
            let _ = Reflect::set(&job, &"starts".into(), &starts);
            let _ = Reflect::set(
                &job,
                &"roundLimit".into(),
                &(config.round_limit as f64).into(),
            );
            jobs.push(&job);
        }
        series.push(Series {
            family: family.to_owned(),
            label: label.to_owned(),
            slot: family_index,
            points: Vec::new(),
        });
    }
    (jobs, series)
}

fn run_sweep(state: &Rc<RefCell<DashState>>) {
    let document = state.borrow().document.clone();
    if state.borrow().busy {
        return;
    }
    let config = read_config(&document);
    if config.families.is_empty() {
        say(&document, "Choose at least one graph class.", "error");
        return;
    }
    let counts = config.agent_counts();
    if counts.is_empty() {
        say(&document, "The agent range is empty.", "error");
        return;
    }
    let (jobs, series) = build_jobs(&config);
    let total = jobs.length();
    {
        let mut app = state.borrow_mut();
        app.config = Some(config.clone());
        app.series = series;
        app.hover = None;
    }
    set_busy(state, true);
    say(
        &document,
        &format!("Running {total} simulations in the worker…"),
        "busy",
    );
    set_text(&document, "runTitle", "Sweeping…");

    create_worker(state);
    let message = js_sys::Object::new();
    let _ = Reflect::set(&message, &"type".into(), &"sweep".into());
    let _ = Reflect::set(&message, &"algorithm".into(), &config.algorithm.into());
    let _ = Reflect::set(&message, &"jobs".into(), &jobs);
    if let Some(worker) = state.borrow().worker.as_ref() {
        let _ = worker.post_message(&message);
    }
}

fn create_worker(state: &Rc<RefCell<DashState>>) {
    if let Some(worker) = state.borrow_mut().worker.take() {
        worker.terminate();
    }
    let url = format!("./worker.js?session={:.0}", js_sys::Date::now());
    let Ok(worker) = Worker::new(&url) else {
        let document = state.borrow().document.clone();
        say(&document, "Could not start the worker.", "error");
        set_busy(state, false);
        return;
    };

    let handler_state = Rc::clone(state);
    let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let data = event.data();
        let kind = Reflect::get(&data, &"type".into())
            .ok()
            .and_then(|value| value.as_string())
            .unwrap_or_default();
        let document = handler_state.borrow().document.clone();
        match kind.as_str() {
            "sweep-progress" => {
                let done = Reflect::get(&data, &"done".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let total = Reflect::get(&data, &"total".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                say(
                    &document,
                    &format!("Ran {done:.0} of {total:.0} simulations…"),
                    "busy",
                );
            }
            "sweep-result" => {
                let rows = Reflect::get(&data, &"results".into()).unwrap_or(JsValue::NULL);
                accept_results(&handler_state, &rows);
            }
            "error" => {
                let message = Reflect::get(&data, &"message".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| "Worker error".into());
                say(&document, &message, "error");
                set_class(&document, "engineState", "engine-state error");
                set_busy(&handler_state, false);
            }
            "ready" => {
                set_text(&document, "engineState", "WASM ready");
                set_class(&document, "engineState", "engine-state ready");
            }
            _ => {}
        }
    });
    worker.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
    onmessage.forget();
    state.borrow_mut().worker = Some(worker);
}

/// Folds the flat result array back into one series per graph class.
fn accept_results(state: &Rc<RefCell<DashState>>, rows: &JsValue) {
    let document = state.borrow().document.clone();
    let Ok(array) = rows.clone().dyn_into::<Array>() else {
        say(&document, "The worker returned no results.", "error");
        set_busy(state, false);
        return;
    };
    let (config, counts) = {
        let app = state.borrow();
        let Some(config) = app.config.clone() else {
            return;
        };
        let counts = config.agent_counts();
        (config, counts)
    };

    let field = match config.metric.as_str() {
        "moves" => "moves",
        "probes" => "probes",
        _ => "rounds",
    };
    let mut index = 0_u32;
    let mut incomplete = 0_usize;
    {
        let mut app = state.borrow_mut();
        for series in &mut app.series {
            series.points.clear();
            for &agents in &counts {
                let row = array.get(index);
                index += 1;
                let value = Reflect::get(&row, &field.into())
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let completed = Reflect::get(&row, &"completed".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !completed {
                    incomplete += 1;
                }
                series.points.push(Point {
                    agents,
                    value,
                    completed,
                });
            }
        }
    }

    set_text(
        &document,
        "runTitle",
        &format!(
            "{} · {} vs agents",
            crate::algorithm_label(&config.algorithm),
            config.metric_label()
        ),
    );
    if incomplete > 0 {
        say(
            &document,
            &format!(
                "Done. {incomplete} run(s) hit the round limit and are drawn as hollow points."
            ),
            "busy",
        );
    } else {
        say(&document, "Done.", "");
    }
    let _ = element::<Element>(&document, "emptyState")
        .class_list()
        .add_1("hidden");
    set_busy(state, false);
    render_table(state);
    render(state);
}

fn series_colors(document: &Document) -> [&'static str; 7] {
    let dark = document
        .document_element()
        .and_then(|root| root.get_attribute("data-theme"))
        .map_or(true, |theme| theme != "light");
    if dark {
        SERIES_DARK
    } else {
        SERIES_LIGHT
    }
}

fn css_var(document: &Document, name: &str, fallback: &str) -> String {
    web_sys::window()
        .and_then(|window| {
            document
                .document_element()
                .and_then(|root| window.get_computed_style(&root).ok().flatten())
                .and_then(|style| style.get_property_value(name).ok())
        })
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

fn resize(state: &Rc<RefCell<DashState>>) {
    let (width, height) = {
        let app = state.borrow();
        let rect = app.canvas.get_bounding_client_rect();
        (rect.width().max(1.0), rect.height().max(1.0))
    };
    let dpr = device_pixel_ratio();
    {
        let app = state.borrow();
        let target_w = ((width * dpr).round() as u32).max(1);
        let target_h = ((height * dpr).round() as u32).max(1);
        if app.canvas.width() != target_w {
            app.canvas.set_width(target_w);
        }
        if app.canvas.height() != target_h {
            app.canvas.set_height(target_h);
        }
    }
    state.borrow_mut().size = (width, height);
    render(state);
}

/// Plot area inset: room for the y tick labels on the left and the x axis
/// labels below.
const PAD_LEFT: f64 = 62.0;
const PAD_RIGHT: f64 = 18.0;
const PAD_TOP: f64 = 18.0;
const PAD_BOTTOM: f64 = 44.0;

#[allow(clippy::too_many_lines)]
fn render(state: &Rc<RefCell<DashState>>) {
    let app = state.borrow();
    let Some(context) = app
        .canvas
        .get_context("2d")
        .ok()
        .flatten()
        .and_then(|value| value.dyn_into::<CanvasRenderingContext2d>().ok())
    else {
        return;
    };
    let (width, height) = app.size;
    let dpr = device_pixel_ratio();
    let _ = context.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    context.clear_rect(0.0, 0.0, width, height);

    let Some(config) = app.config.as_ref() else {
        return;
    };
    if app.series.is_empty() {
        return;
    }

    let document = &app.document;
    let grid = css_var(document, "--border", "#263653");
    let muted = css_var(document, "--muted", "#91a0bb");
    let text = css_var(document, "--text", "#e8eef9");
    let surface = css_var(document, "--canvas", "#0e1628");
    let colors = series_colors(document);

    let counts = config.agent_counts();
    let (Some(&first), Some(&last)) = (counts.first(), counts.last()) else {
        return;
    };
    let data_max = app
        .series
        .iter()
        .flat_map(|series| series.points.iter())
        .map(|point| point.value)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let step = nice_step(data_max / 4.0);
    let ticks = (data_max / step).ceil().max(1.0);
    let max_value = step * ticks;

    // Direct labels sit past the last point, so the plot has to give up the
    // room they need or they run off the edge.
    context.set_font("600 11px Inter, ui-sans-serif, system-ui, sans-serif");
    let label_room = if app.series.len() <= 4 {
        app.series
            .iter()
            .map(|series| {
                context
                    .measure_text(&series.label)
                    .map_or(60.0, |metrics| metrics.width())
            })
            .fold(0.0_f64, f64::max)
            + 14.0
    } else {
        0.0
    };
    let pad_right = PAD_RIGHT + label_room;

    let plot_w = (width - PAD_LEFT - pad_right).max(1.0);
    let plot_h = (height - PAD_TOP - PAD_BOTTOM).max(1.0);
    let span = ((last - first) as f64).max(1.0);
    let x_of = |agents: usize| PAD_LEFT + plot_w * ((agents - first) as f64) / span;
    let y_of = |value: f64| PAD_TOP + plot_h * (1.0 - value / max_value);

    // Grid and axes, recessive.
    context.set_font("11px Inter, ui-sans-serif, system-ui, sans-serif");
    context.set_line_width(1.0);
    context.set_stroke_style_str(&grid);
    context.set_fill_style_str(&muted);
    context.set_text_align("right");
    context.set_text_baseline("middle");
    let tick_count = ticks as usize;
    for index in 0..=tick_count {
        let value = step * index as f64;
        let y = y_of(value).round() + 0.5;
        context.begin_path();
        context.move_to(PAD_LEFT, y);
        context.line_to(PAD_LEFT + plot_w, y);
        context.stroke();
        let _ = context.fill_text(&format_value(value), PAD_LEFT - 10.0, y);
    }

    // x ticks: thin them out so labels never collide.
    context.set_text_align("center");
    context.set_text_baseline("top");
    let max_ticks = (plot_w / 52.0).floor().max(2.0) as usize;
    let stride = counts.len().div_ceil(max_ticks).max(1);
    for (index, &agents) in counts.iter().enumerate() {
        if index % stride != 0 && index + 1 != counts.len() {
            continue;
        }
        let _ = context.fill_text(&agents.to_string(), x_of(agents), PAD_TOP + plot_h + 10.0);
    }

    context.set_fill_style_str(&muted);
    context.set_text_align("center");
    let _ = context.fill_text("agents (k)", PAD_LEFT + plot_w / 2.0, height - 16.0);
    context.save();
    let _ = context.translate(14.0, PAD_TOP + plot_h / 2.0);
    let _ = context.rotate(-std::f64::consts::FRAC_PI_2);
    let _ = context.fill_text(config.metric_label(), 0.0, 0.0);
    context.restore();

    // Series: 2px lines, markers ringed in the surface colour so overlaps stay
    // readable, hollow when the run hit its round limit.
    context.set_line_join("round");
    context.set_line_cap("round");
    for series in &app.series {
        let color = colors[series.slot % colors.len()];
        context.set_stroke_style_str(color);
        context.set_line_width(2.0);
        context.begin_path();
        for (index, point) in series.points.iter().enumerate() {
            let (x, y) = (x_of(point.agents), y_of(point.value));
            if index == 0 {
                context.move_to(x, y);
            } else {
                context.line_to(x, y);
            }
        }
        context.stroke();

        for point in &series.points {
            let (x, y) = (x_of(point.agents), y_of(point.value));
            context.begin_path();
            let _ = context.arc(x, y, 4.0, 0.0, std::f64::consts::TAU);
            context.set_fill_style_str(if point.completed { color } else { &surface });
            context.fill();
            context.set_line_width(2.0);
            context.set_stroke_style_str(if point.completed { &surface } else { color });
            context.stroke();
        }
    }

    // Direct labels only when there are few enough series to place them without
    // collisions; the legend carries identity in every case.
    if app.series.len() <= 4 {
        context.set_text_align("left");
        context.set_text_baseline("middle");
        context.set_font("600 11px Inter, ui-sans-serif, system-ui, sans-serif");
        // Two classes that finish at nearly the same cost would print on top of
        // each other, so the labels are pushed apart before anything is drawn.
        let mut placed: Vec<(f64, f64, &str, &str)> = app
            .series
            .iter()
            .filter_map(|series| {
                let point = series.points.last()?;
                Some((
                    x_of(point.agents) + 9.0,
                    y_of(point.value),
                    series.label.as_str(),
                    colors[series.slot % colors.len()],
                ))
            })
            .collect();
        placed.sort_by(|a, b| a.1.total_cmp(&b.1));
        const LABEL_GAP: f64 = 13.0;
        for index in 1..placed.len() {
            let previous = placed[index - 1].1;
            if placed[index].1 - previous < LABEL_GAP {
                placed[index].1 = previous + LABEL_GAP;
            }
        }
        // Keep the whole stack inside the plot if pushing down ran past it.
        if let Some(last) = placed.last() {
            let overflow = last.1 - (PAD_TOP + plot_h);
            if overflow > 0.0 {
                for entry in &mut placed {
                    entry.1 -= overflow;
                }
            }
        }
        for (x, y, label, color) in placed {
            context.set_fill_style_str(color);
            let _ = context.fill_text(label, x, y);
        }
    }

    // Crosshair and readout for the hovered agent count.
    if let Some(hover) = app.hover.and_then(|index| counts.get(index).copied()) {
        let x = x_of(hover).round() + 0.5;
        context.set_stroke_style_str(&muted);
        context.set_line_width(1.0);
        context.begin_path();
        context.move_to(x, PAD_TOP);
        context.line_to(x, PAD_TOP + plot_h);
        context.stroke();
        context.set_fill_style_str(&text);
        context.set_text_align("center");
        context.set_text_baseline("bottom");
        context.set_font("600 11px Inter, ui-sans-serif, system-ui, sans-serif");
        let _ = context.fill_text(&format!("k = {hover}"), x, PAD_TOP - 4.0);
    }
}

/// A round tick step at or above `rough`, from the 1 / 2 / 2.5 / 5 ladder.
///
/// Dividing the maximum into four gives ticks like 153.8; readers want 150.
fn nice_step(rough: f64) -> f64 {
    if rough <= 0.0 {
        return 1.0;
    }
    let magnitude = 10.0_f64.powf(rough.log10().floor());
    let normalized = rough / magnitude;
    let step = if normalized <= 1.0 {
        1.0
    } else if normalized <= 2.0 {
        2.0
    } else if normalized <= 2.5 {
        2.5
    } else if normalized <= 5.0 {
        5.0
    } else {
        10.0
    };
    step * magnitude
}

fn format_value(value: f64) -> String {
    if value >= 10_000.0 {
        format!("{:.0}k", value / 1000.0)
    } else if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// The table view. Always present: it is the relief for the light-mode contrast
/// warning, and it is the form researchers actually read numbers out of.
fn render_table(state: &Rc<RefCell<DashState>>) {
    let app = state.borrow();
    let document = &app.document;
    let Some(config) = app.config.as_ref() else {
        return;
    };
    let counts = config.agent_counts();
    let colors = series_colors(document);

    let mut html = String::from("<table><thead><tr><th>Graph class</th>");
    for agents in &counts {
        html.push_str(&format!("<th>k={agents}</th>"));
    }
    html.push_str("</tr></thead><tbody>");
    for series in &app.series {
        html.push_str(&format!(
            "<tr><th><i class=\"swatch\" style=\"background:{}\"></i>{}</th>",
            colors[series.slot % colors.len()],
            series.label
        ));
        for point in &series.points {
            html.push_str(&format!(
                "<td{}>{}</td>",
                if point.completed {
                    ""
                } else {
                    " class=\"capped\""
                },
                format_value(point.value)
            ));
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    element::<Element>(document, "resultsTable").set_inner_html(&html);

    // Legend: identity is never colour alone, so every entry is labelled.
    let mut legend = String::new();
    for series in &app.series {
        legend.push_str(&format!(
            "<span><i class=\"swatch\" style=\"background:{}\"></i>{}</span>",
            colors[series.slot % colors.len()],
            series.label
        ));
    }
    element::<Element>(document, "chartLegend").set_inner_html(&legend);
}

fn export_csv(state: &Rc<RefCell<DashState>>) {
    let app = state.borrow();
    let Some(config) = app.config.as_ref() else {
        return;
    };
    let mut csv = String::from("algorithm,graph_class,nodes,agents,seed,metric,value,completed\n");
    for series in &app.series {
        for point in &series.points {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                config.algorithm,
                series.family,
                config.nodes,
                point.agents,
                config.seed,
                config.metric,
                point.value,
                point.completed
            ));
        }
    }
    let parts = Array::new();
    parts.push(&JsValue::from_str(&csv));
    let Ok(blob) = Blob::new_with_str_sequence(&parts) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) else {
        return;
    };
    let Ok(anchor) = app
        .document
        .create_element("a")
        .and_then(|element| element.dyn_into::<HtmlAnchorElement>().map_err(Into::into))
    else {
        return;
    };
    let _ = anchor.set_attribute("href", &url);
    let _ = anchor.set_attribute(
        "download",
        &format!("ccm-sweep-{}-{}.csv", config.algorithm, config.metric),
    );
    if let Some(body) = app.document.body() {
        let _ = body.append_child(&anchor);
    }
    anchor.click();
    anchor.remove();
    let _ = web_sys::Url::revoke_object_url(&url);
}

fn hover(state: &Rc<RefCell<DashState>>, event: &MouseEvent) {
    let nearest = {
        let app = state.borrow();
        let Some(config) = app.config.as_ref() else {
            return;
        };
        let counts = config.agent_counts();
        if counts.is_empty() {
            return;
        }
        let rect = app.canvas.get_bounding_client_rect();
        let x = f64::from(event.client_x()) - rect.left();
        let (width, _) = app.size;
        // Mirrors render's reserved label room so the crosshair lands on the
        // point the reader is pointing at.
        let label_room = if app.series.len() <= 4 { 74.0 } else { 0.0 };
        let plot_w = (width - PAD_LEFT - PAD_RIGHT - label_room).max(1.0);
        let ratio = ((x - PAD_LEFT) / plot_w).clamp(0.0, 1.0);
        let index = (ratio * (counts.len() - 1) as f64).round() as usize;
        Some(index.min(counts.len() - 1))
    };
    if state.borrow().hover != nearest {
        state.borrow_mut().hover = nearest;
        render(state);
    }
}

/// Entry point for `dashboard.html`.
///
/// # Errors
///
/// Returns an error when the window or document is unavailable.
#[wasm_bindgen]
pub fn start_dashboard() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("window unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("document unavailable"))?;
    let canvas: HtmlCanvasElement = element(&document, "chartCanvas");
    let state = Rc::new(RefCell::new(DashState {
        document: document.clone(),
        canvas,
        worker: None,
        observer: None,
        busy: false,
        config: None,
        series: Vec::new(),
        hover: None,
        size: (1.0, 1.0),
    }));

    crate::restore_theme(&document);
    crate::hook_theme_toggle(&document, {
        let state = Rc::clone(&state);
        move || {
            render_table(&state);
            render(&state);
        }
    });

    {
        let run_state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut()>::new(move || run_sweep(&run_state));
        let _ = element::<HtmlElement>(&document, "run")
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    {
        let csv_state = Rc::clone(&state);
        let closure = Closure::<dyn FnMut()>::new(move || export_csv(&csv_state));
        let _ = element::<HtmlElement>(&document, "exportCsv")
            .add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    {
        let family: web_sys::HtmlSelectElement = element(&document, "algorithm");
        let document = document.clone();
        let closure = Closure::<dyn FnMut()>::new(move || {
            let algorithm = select_value(&document, "algorithm");
            set_text(
                &document,
                "algorithmNote",
                if algorithm == "p1tree" {
                    "P1Tree requires a rooted start; every sweep places all agents at node 0."
                } else {
                    "All agents start at node 0, so the classes are compared on the same placement."
                },
            );
        });
        let _ = family.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref());
        closure.forget();
    }
    {
        let hover_state = Rc::clone(&state);
        let closure =
            Closure::<dyn FnMut(MouseEvent)>::new(move |event| hover(&hover_state, &event));
        let _ = element::<HtmlElement>(&document, "chartCanvas")
            .add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    let observer_state = Rc::clone(&state);
    let observer_callback =
        Closure::<dyn FnMut(Array, ResizeObserver)>::new(move |_, _| resize(&observer_state));
    let observer = ResizeObserver::new(observer_callback.as_ref().unchecked_ref())?;
    observer.observe(&element::<Element>(&document, "chartWrap"));
    observer_callback.forget();
    state.borrow_mut().observer = Some(observer);

    let resize_state = Rc::clone(&state);
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| resize(&resize_state));
    window.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref())?;
    closure.forget();

    create_worker(&state);
    resize(&state);
    Ok(())
}
