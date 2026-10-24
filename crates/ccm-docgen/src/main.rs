//! Generates `docs.html` — the flowcharts as themed SVG, with an explanation of
//! every box and the source of every function a box stands for — and, from the
//! same chart data, `docs/algorithm-flowcharts.md` for GitHub.
//!
//! The SVG is emitted here rather than by a diagram library so that it can be
//! written against the application's CSS variables — the diagrams then follow
//! the light/dark toggle instead of baking one theme in — and so that every box
//! carries a stable id the page can hang a click handler on. It also keeps the
//! page free of a megabyte of renderer.
//!
//! Run `./scripts/gen-docs.sh` (or `./scripts/gen-flowcharts.sh`, the same thing)
//! rather than this binary directly.

mod code;
mod markdown;
mod spec;
mod svg;
mod text;

use spec::{Chart, Shape, CHARTS};
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use text::{escape, inline, json_str, json_strings, paragraphs};

/// Where the "GitHub" button of a code block points.
const REPO_BLOB: &str = "https://github.com/drdebmath/CCMModel/blob/main";

/// The order the guided tour visits a chart's boxes in: the flow from the
/// entry, breadth first, with each helper right after the step that calls it.
fn tour(chart: &Chart) -> Vec<&'static str> {
    let mut order = Vec::new();
    let mut queue = VecDeque::from([chart.nodes[0].id]);
    while let Some(id) = queue.pop_front() {
        if order.contains(&id) {
            continue;
        }
        order.push(id);
        for edge in chart.edges.iter().filter(|e| e.from == id && e.aside) {
            if !order.contains(&edge.to) {
                order.push(edge.to);
            }
        }
        for edge in chart.edges.iter().filter(|e| e.from == id && !e.aside) {
            queue.push_back(edge.to);
        }
    }
    for node in chart.nodes {
        if !order.contains(&node.id) {
            order.push(node.id);
        }
    }
    order
}

fn legend() -> String {
    let rows = [
        (Shape::Entry, "Oval: where the chart starts"),
        (Shape::Step, "Rectangle: a step"),
        (Shape::Decision, "Diamond: a question with branches"),
        (Shape::Terminal, "Oval: how a run ends"),
        (Shape::Aside, "Dashed box: a helper the step calls"),
    ];
    let mut out = String::new();
    for (shape, label) in rows {
        let _ = write!(
            out,
            r#"<li><svg width="18" height="18" viewBox="-9 -9 18 18" class="k-{}">{}</svg>{label}</li>"#,
            svg::kind(shape),
            svg::icon(shape)
        );
    }
    out.push_str(r#"<li><span class="tag">&lt;/&gt;</span>Opens the function's code</li>"#);
    out
}

/// One function's entry in the page data: its location, a GitHub link and the
/// highlighted lines (the page copies their text).
fn code_json(chart: &Chart, name: &str, found: &code::Extract) -> String {
    let lines = code::highlight(&found.text);
    let end = found.line + lines.len() - 1;
    format!(
        r#"{}:{{"name":{},"path":{},"line":{},"url":{},"html":{}}}"#,
        json_str(name),
        json_str(name.split('@').next().unwrap_or(name)),
        json_str(chart.source),
        found.line,
        json_str(&format!(
            "{REPO_BLOB}/{}#L{}-L{end}",
            chart.source, found.line
        )),
        json_strings(lines.iter().map(String::as_str))
    )
}

/// One node's entry in the page data. `code` names an entry of the chart's
/// code table, so a function several boxes stand for is stored once.
fn node_json(chart: &Chart, node: &spec::Node, drawn: &svg::Drawn) -> String {
    let ins: Vec<&str> = chart
        .edges
        .iter()
        .filter(|e| e.to == node.id)
        .map(|e| e.from)
        .collect();
    let outs: Vec<&str> = chart
        .edges
        .iter()
        .filter(|e| e.from == node.id)
        .map(|e| e.to)
        .collect();
    let labels: Vec<String> = chart
        .edges
        .iter()
        .filter(|e| e.from == node.id)
        .filter_map(|e| {
            e.label
                .map(|label| format!("{}:{}", json_str(e.to), json_str(label)))
        })
        .collect();
    let (x, y, w, h) = drawn.boxes[node.id];
    format!(
        r#"{}:{{"title":{},"sub":{},"kind":{},"desc":{},"in":{},"out":{},"labels":{{{}}},"box":[{x:.1},{y:.1},{w:.1},{h:.1}],"code":{}}}"#,
        json_str(node.id),
        json_str(node.title),
        json_str(node.sub),
        json_str(svg::kind(node.shape)),
        json_str(&inline(node.desc)),
        json_strings(ins),
        json_strings(outs),
        labels.join(","),
        node.func.map_or_else(|| "null".to_owned(), json_str)
    )
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf();

    let mut nav = String::new();
    let mut sections = String::new();
    let mut data = Vec::new();
    let mut missing = Vec::new();
    let mut sources = Vec::new();

    for (number, chart) in CHARTS.iter().enumerate() {
        let source = fs::read_to_string(root.join(chart.source)).expect("chart source");
        sources.push(source.clone());
        let drawn = svg::draw(chart);
        let _ = write!(
            nav,
            r##"<a class="nav-item" href="#{id}" data-chart="{id}"><span class="nav-num">{}</span><span><span class="nav-name">{}</span><span class="nav-short">{}</span></span></a>"##,
            number + 1,
            escape(chart.title),
            escape(chart.short),
            id = chart.id
        );
        let mut notes = String::new();
        for note in chart.notes {
            let _ = write!(notes, "<li>{}</li>", inline(note));
        }
        let _ = write!(
            sections,
            r#"<section class="chart-panel" id="panel-{id}" data-chart="{id}" hidden>
<div class="chart-head"><p class="eyebrow">Chart {} of {} · <code>{}</code></p><h1>{}</h1><div class="blurb">{}</div></div>
<div class="stage-wrap"><div class="stage">{}</div>
<div class="stage-tools"><button class="primary tour-start" type="button">▶ Guided tour</button><span class="stage-hint">Click a box for details · drag to move · Ctrl + scroll to zoom</span><button class="icon-button" type="button" data-zoom="out" aria-label="Zoom out">−</button><output class="zoom-level">100%</output><button class="icon-button" type="button" data-zoom="in" aria-label="Zoom in">+</button><button class="secondary" type="button" data-zoom="fit">Fit</button></div></div>
<details class="notes" open><summary>Notes</summary><ul>{notes}</ul></details>
</section>
"#,
            number + 1,
            CHARTS.len(),
            escape(chart.crate_name),
            escape(chart.title),
            paragraphs(chart.intro),
            drawn.svg,
            id = chart.id
        );
        let entries: Vec<String> = chart
            .nodes
            .iter()
            .map(|node| node_json(chart, node, &drawn))
            .collect();
        let mut functions: Vec<&str> = chart.nodes.iter().filter_map(|node| node.func).collect();
        functions.sort_unstable();
        functions.dedup();
        let mut code = Vec::new();
        for name in functions {
            match code::extract(&source, name) {
                Some(found) => code.push(code_json(chart, name, &found)),
                None => missing.push(format!("{}::{name}", chart.crate_name)),
            }
        }
        data.push(format!(
            r#"{}:{{"title":{},"order":{},"nodes":{{{}}},"code":{{{}}}}}"#,
            json_str(chart.id),
            json_str(chart.title),
            json_strings(tour(chart)),
            entries.join(","),
            code.join(",")
        ));
    }

    assert!(
        missing.is_empty(),
        "these functions are named by a chart but were not found in its source: {missing:?}"
    );

    let page = include_str!("page.html")
        .replace("<!--NAV-->", &nav)
        .replace("<!--LEGEND-->", &legend())
        .replace("<!--SECTIONS-->", &sections)
        .replace(
            "<!--DATA-->",
            &format!(r#"{{"charts":{{{}}}}}"#, data.join(",")),
        );
    let out = root.join("docs.html");
    fs::write(&out, page).expect("write docs.html");
    println!("wrote {}", out.display());

    let doc = root.join("docs/algorithm-flowcharts.md");
    fs::write(&doc, markdown::document(&sources)).expect("write docs/algorithm-flowcharts.md");
    println!("wrote {}", doc.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tour_visits_every_box_once_starting_at_the_entry() {
        for chart in CHARTS {
            let order = tour(chart);
            assert_eq!(order[0], chart.nodes[0].id);
            let mut sorted = order.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), chart.nodes.len(), "{}", chart.id);
        }
    }

    #[test]
    fn every_named_function_exists_in_its_crate() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for chart in CHARTS {
            let source = fs::read_to_string(root.join(chart.source)).expect("chart source");
            for node in chart.nodes {
                if let Some(name) = node.func {
                    assert!(
                        code::extract(&source, name).is_some(),
                        "{}::{name}",
                        chart.crate_name
                    );
                }
            }
        }
    }
}
