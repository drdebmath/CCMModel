//! Generates `docs.html`: the flowcharts as themed SVG, with the source of
//! every function a box stands for.
//!
//! The SVG is emitted here rather than by a diagram library so that it can be
//! written against the application's CSS variables — the diagrams then follow
//! the light/dark toggle instead of baking one theme in — and so that every box
//! carries a stable id the page can hang a click handler on. It also keeps the
//! page free of a megabyte of renderer.
//!
//! Run `./scripts/gen-docs.sh` rather than this binary directly.

mod spec;

use spec::{Chart, Edge, Node, Route, Shape, CHARTS};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Grid unit in pixels.
const COL: f64 = 260.0;
const ROW: f64 = 92.0;
const BOX_W: f64 = 208.0;
const BOX_H: f64 = 46.0;
const DIAMOND_W: f64 = 224.0;
const DIAMOND_H: f64 = 62.0;
const MARGIN: f64 = 40.0;

struct Placed<'a> {
    node: &'a Node,
    cx: f64,
    cy: f64,
    half_w: f64,
    half_h: f64,
}

impl Placed<'_> {
    /// Where an edge leaving toward `(tx, ty)` should start.
    ///
    /// A diamond is treated as its bounding box, which is close enough at this
    /// size and keeps arrowheads off the corners.
    fn anchor(&self, tx: f64, ty: f64) -> (f64, f64) {
        let (dx, dy) = (tx - self.cx, ty - self.cy);
        if dx.abs() * self.half_h > dy.abs() * self.half_w {
            let x = self.cx + self.half_w * dx.signum();
            (x, self.cy + dy * (self.half_w / dx.abs()).min(1.0) * 0.35)
        } else {
            let y = self.cy + self.half_h * dy.signum();
            (self.cx + dx * 0.18, y)
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn place(chart: &Chart) -> Vec<Placed<'_>> {
    chart
        .nodes
        .iter()
        .map(|node| {
            let diamond = node.shape == Shape::Decision;
            Placed {
                node,
                cx: node.col * COL,
                cy: node.row * ROW,
                half_w: if diamond { DIAMOND_W } else { BOX_W } / 2.0,
                half_h: if diamond { DIAMOND_H } else { BOX_H } / 2.0,
            }
        })
        .collect()
}

fn shape_svg(placed: &Placed<'_>) -> String {
    let (cx, cy, hw, hh) = (placed.cx, placed.cy, placed.half_w, placed.half_h);
    match placed.node.shape {
        Shape::Decision => format!(
            r#"<polygon points="{cx},{} {},{cy} {cx},{} {},{cy}"/>"#,
            cy - hh,
            cx + hw,
            cy + hh,
            cx - hw
        ),
        Shape::Entry | Shape::Terminal => format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}"/>"#,
            cx - hw,
            cy - hh,
            hw * 2.0,
            hh * 2.0,
            hh
        ),
        Shape::Step | Shape::Aside => format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="9"/>"#,
            cx - hw,
            cy - hh,
            hw * 2.0,
            hh * 2.0
        ),
    }
}

fn class_of(node: &Node) -> String {
    let base = match node.shape {
        Shape::Entry => "entry",
        Shape::Step => "step",
        Shape::Decision => "decision",
        Shape::Terminal => "terminal",
        Shape::Aside => "aside",
    };
    if node.func.is_some() {
        format!("node {base} has-code")
    } else {
        format!("node {base}")
    }
}

fn edge_path(from: &Placed<'_>, to: &Placed<'_>, edge: &Edge) -> (String, f64, f64) {
    match edge.route {
        Route::Direct => {
            let (sx, sy) = from.anchor(to.cx, to.cy);
            let (tx, ty) = to.anchor(from.cx, from.cy);
            (
                format!("M {sx:.1} {sy:.1} L {tx:.1} {ty:.1}"),
                (sx + tx) / 2.0,
                (sy + ty) / 2.0,
            )
        }
        Route::Elbow => {
            let sy = from.cy + from.half_h;
            let ty = to.cy - to.half_h;
            let mid = (sy + ty) / 2.0;
            (
                format!(
                    "M {:.1} {sy:.1} L {:.1} {mid:.1} L {:.1} {mid:.1} L {:.1} {ty:.1}",
                    from.cx, from.cx, to.cx, to.cx
                ),
                (from.cx + to.cx) / 2.0,
                mid,
            )
        }
        Route::Around(side) => {
            // Out of the side, along a lane clear of the widest box involved,
            // and back in. The lane has to start beyond that box or the edge
            // label lands on top of it; the sign picks which side to leave from,
            // and the magnitude separates lanes that would otherwise overlap.
            let dir = if side >= 0 { 1.0 } else { -1.0 };
            let widest = from.half_w.max(to.half_w);
            let base = if dir > 0.0 {
                from.cx.max(to.cx)
            } else {
                from.cx.min(to.cx)
            };
            let lane = base + dir * (widest + COL * 0.16 * f64::from(side.abs()));
            let sx = from.cx + from.half_w * dir;
            let tx = to.cx + to.half_w * dir;
            (
                format!(
                    "M {sx:.1} {:.1} L {lane:.1} {:.1} L {lane:.1} {:.1} L {tx:.1} {:.1}",
                    from.cy, from.cy, to.cy, to.cy
                ),
                lane,
                (from.cy + to.cy) / 2.0,
            )
        }
    }
}

fn svg_for(chart: &Chart) -> String {
    let placed = place(chart);
    let index: BTreeMap<&str, &Placed<'_>> =
        placed.iter().map(|entry| (entry.node.id, entry)).collect();

    let min_x = placed
        .iter()
        .map(|p| p.cx - p.half_w)
        .fold(f64::INFINITY, f64::min);
    let max_x = placed
        .iter()
        .map(|p| p.cx + p.half_w)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = placed
        .iter()
        .map(|p| p.cy - p.half_h)
        .fold(f64::INFINITY, f64::min);
    let max_y = placed
        .iter()
        .map(|p| p.cy + p.half_h)
        .fold(f64::NEG_INFINITY, f64::max);
    // Loop-back lanes and their labels sit outside the boxes.
    let pad = COL * 0.62;
    let (x0, y0) = (min_x - pad, min_y - MARGIN);
    let (w, h) = (max_x - min_x + pad * 2.0, max_y - min_y + MARGIN * 2.0);

    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg class="chart" viewBox="{x0:.0} {y0:.0} {w:.0} {h:.0}" role="img" aria-label="{} flowchart" xmlns="http://www.w3.org/2000/svg">
<defs><marker id="arrow-{}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z"/></marker></defs>
<g class="edges">
"#,
        escape(chart.title),
        chart.id
    );

    for edge in chart.edges {
        let (Some(from), Some(to)) = (index.get(edge.from), index.get(edge.to)) else {
            continue;
        };
        let (path, lx, ly) = edge_path(from, to, edge);
        let class = if edge.aside { "edge aside" } else { "edge" };
        let _ = write!(
            out,
            r#"<path class="{class}" d="{path}" marker-end="url(#arrow-{})"/>"#,
            chart.id
        );
        if let Some(label) = edge.label {
            let _ = write!(
                out,
                r#"<text class="edge-label" x="{lx:.1}" y="{ly:.1}">{}</text>"#,
                escape(label)
            );
        }
        out.push('\n');
    }
    out.push_str("</g>\n<g class=\"nodes\">\n");

    for entry in &placed {
        let node = entry.node;
        let code_attr = node
            .func
            .map(|name| format!(r#" data-fn="{name}" tabindex="0" role="button""#))
            .unwrap_or_default();
        let _ = write!(
            out,
            r#"<g id="{}-{}" class="{}"{code_attr}>{}"#,
            chart.id,
            node.id,
            class_of(node),
            shape_svg(entry)
        );
        let lines: Vec<&str> = node.label.split('|').collect();
        let count = u32::try_from(lines.len()).unwrap_or(1);
        let start = entry.cy - (f64::from(count) - 1.0) * 8.0;
        for (index, line) in lines.iter().enumerate() {
            let _ = write!(
                out,
                r#"<text x="{:.1}" y="{:.1}">{}</text>"#,
                entry.cx,
                f64::from(u32::try_from(index).unwrap_or(0)).mul_add(16.0, start),
                escape(line)
            );
        }
        out.push_str("</g>\n");
    }
    out.push_str("</g>\n</svg>\n");
    out
}

/// Pulls a function out of a source file.
///
/// rustfmt puts a function's closing brace at the same indentation as its `fn`,
/// which makes this exact where brace counting would be defeated by a brace
/// inside a string literal.
fn extract(source: &str, name: &str) -> Option<String> {
    let mut lines = source.lines().peekable();
    let mut doc: Vec<&str> = Vec::new();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("///") || trimmed.starts_with("#[") {
            doc.push(line);
            continue;
        }
        let is_fn = trimmed.starts_with(&format!("fn {name}"))
            || trimmed.starts_with(&format!("pub fn {name}"));
        let boundary = is_fn && trimmed[trimmed.find(name)? + name.len()..].starts_with(['<', '(']);
        if !boundary {
            doc.clear();
            continue;
        }
        let indent = &line[..line.len() - trimmed.len()];
        let closing = format!("{indent}}}");
        let mut body: Vec<&str> = doc.clone();
        body.push(line);
        for next in lines.by_ref() {
            body.push(next);
            if next == closing {
                return Some(body.join("\n"));
            }
        }
        return None;
    }
    None
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf();

    let mut sections = String::new();
    let mut code_blocks = String::new();
    let mut nav = String::new();
    let mut missing = Vec::new();

    for chart in CHARTS {
        let source = fs::read_to_string(root.join(chart.source)).expect("chart source");
        let _ = write!(
            nav,
            r#"<button class="chart-tab" type="button" data-chart="{}">{}</button>"#,
            chart.id,
            escape(chart.title)
        );
        let _ = write!(
            sections,
            r#"<section class="chart-panel hidden" id="panel-{}" data-crate="{}">
<p class="chart-blurb">{}</p>
<div class="chart-frame"><div class="chart-stage">{}</div></div>
</section>
"#,
            chart.id,
            escape(chart.crate_name),
            escape(chart.blurb),
            svg_for(chart)
        );

        for node in chart.nodes {
            let Some(name) = node.func else { continue };
            match extract(&source, name) {
                Some(code) => {
                    let _ = write!(
                        code_blocks,
                        r#"<template data-code="{name}" data-source="{}">{}</template>"#,
                        escape(chart.source),
                        escape(&code)
                    );
                }
                None => missing.push(format!("{}::{name}", chart.crate_name)),
            }
        }
    }

    assert!(
        missing.is_empty(),
        "these functions are named by a chart but were not found in its source: {missing:?}"
    );

    let template = include_str!("page.html");
    let page = template
        .replace("<!--NAV-->", &nav)
        .replace("<!--SECTIONS-->", &sections)
        .replace("<!--CODE-->", &code_blocks);
    let out = root.join("docs.html");
    fs::write(&out, page).expect("write docs.html");
    println!("wrote {}", out.display());
}
