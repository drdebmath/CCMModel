//! Lays a chart out and draws it as SVG written against the page's CSS
//! variables, so the diagrams follow the light/dark toggle.
//!
//! Every box is a card: a coloured bar and a small icon in the shape of its
//! kind (oval, rectangle, diamond, dashed box), a title in plain words and the
//! function under it. Edges are orthogonal with rounded corners, and their
//! labels sit in chips so they never collide with a line.

use crate::spec::{Chart, Edge, Lanes, Node, Route, Shape};
use crate::text::escape;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Grid unit in pixels.
const COL: f64 = 320.0;
const ROW: f64 = 104.0;
const CARD_W: f64 = 264.0;
const CARD_H: f64 = 58.0;
/// Extra height for a title on two lines.
const LINE_H: f64 = 16.0;
/// Room for this many title characters on one line.
const TITLE_CHARS: usize = 25;
/// Distance between loop-back lanes.
const LANE_GAP: f64 = 48.0;
const MARGIN: f64 = 28.0;
const CORNER: f64 = 10.0;

pub fn kind(shape: Shape) -> &'static str {
    match shape {
        Shape::Entry => "entry",
        Shape::Step => "step",
        Shape::Decision => "decision",
        Shape::Terminal => "end",
        Shape::Aside => "aside",
    }
}

/// The icon for a kind: a small copy of its classic flowchart shape.
pub fn icon(shape: Shape) -> &'static str {
    match shape {
        Shape::Entry | Shape::Terminal => {
            r#"<rect x="-7.5" y="-4.5" width="15" height="9" rx="4.5" class="ic"/>"#
        }
        Shape::Step => r#"<rect x="-7" y="-4.8" width="14" height="9.6" rx="1" class="ic"/>"#,
        Shape::Decision => r#"<path d="M0,-7.2 L7.2,0 L0,7.2 L-7.2,0 Z" class="ic"/>"#,
        Shape::Aside => {
            r#"<rect x="-7" y="-4.8" width="14" height="9.6" rx="1" class="ic" stroke-dasharray="2.4 1.8"/>"#
        }
    }
}

/// Splits a title into at most two lines of about `TITLE_CHARS` characters.
fn wrap(title: &str) -> Vec<String> {
    if title.chars().count() <= TITLE_CHARS {
        return vec![title.to_owned()];
    }
    let words: Vec<&str> = title.split(' ').collect();
    let mut best = 1;
    let mut best_cost = usize::MAX;
    for split in 1..words.len() {
        let a = words[..split].join(" ").chars().count();
        let b = words[split..].join(" ").chars().count();
        let cost = a.max(b);
        if cost < best_cost {
            best_cost = cost;
            best = split;
        }
    }
    vec![words[..best].join(" "), words[best..].join(" ")]
}

pub struct Placed<'a> {
    pub node: &'a Node,
    pub cx: f64,
    pub cy: f64,
    pub hw: f64,
    pub hh: f64,
    lines: Vec<String>,
}

impl Placed<'_> {
    fn top(&self) -> (f64, f64) {
        (self.cx, self.cy - self.hh)
    }
    fn bottom(&self) -> (f64, f64) {
        (self.cx, self.cy + self.hh)
    }
    fn side(&self, dir: f64) -> (f64, f64) {
        (self.cx + self.hw * dir, self.cy)
    }
}

pub fn place(chart: &Chart) -> Vec<Placed<'_>> {
    chart
        .nodes
        .iter()
        .map(|node| {
            let lines = wrap(node.title);
            let extra = if lines.len() > 1 { LINE_H } else { 0.0 };
            Placed {
                node,
                cx: node.col * COL,
                cy: node.row * ROW,
                hw: CARD_W / 2.0,
                hh: (CARD_H + extra) / 2.0,
                lines,
            }
        })
        .collect()
}

/// Edge tone, from its label: yes-branches are drawn green, no-branches dashed.
pub fn tone(edge: &Edge) -> &'static str {
    if edge.aside {
        return "aside";
    }
    match edge
        .label
        .map(|label| label.split([' ', ',']).next().unwrap_or(""))
    {
        Some("yes") => "yes",
        Some("no" | "none" | "not") => "no",
        _ => "plain",
    }
}

struct Route2 {
    points: Vec<(f64, f64)>,
    /// Where the label chip goes.
    label_at: (f64, f64),
}

fn route(from: &Placed<'_>, to: &Placed<'_>, edge: &Edge, extent: (f64, f64)) -> Route2 {
    let (dx, dy) = (to.cx - from.cx, to.cy - from.cy);
    match edge.route {
        Route::Auto if dx.abs() < 1.0 => {
            let (start, end) = if dy > 0.0 {
                (from.bottom(), to.top())
            } else {
                (from.top(), to.bottom())
            };
            let near =
                start.1 + (end.1 - start.1).signum() * 15.0_f64.min((end.1 - start.1).abs() / 2.0);
            Route2 {
                points: vec![start, end],
                label_at: (start.0, near),
            }
        }
        Route::Auto if overlap(from, to).is_some() && dx.abs() >= 1.0 => {
            // Side by side, even if not on one row: straight across, at a
            // height both boxes share.
            let y = overlap(from, to).unwrap_or(from.cy);
            let dir = dx.signum();
            let (start, end) = ((from.side(dir).0, y), (to.side(-dir).0, y));
            Route2 {
                points: vec![start, end],
                label_at: ((start.0 + end.0) / 2.0, y),
            }
        }
        Route::Auto => {
            // Across out of the source's side, then down (or up) into the target.
            let start = from.side(dx.signum());
            let corner = (to.cx, from.cy);
            let end = if dy > 0.0 { to.top() } else { to.bottom() };
            Route2 {
                points: vec![start, corner, end],
                label_at: ((start.0 + corner.0) / 2.0, start.1),
            }
        }
        Route::Elbow => {
            let start = from.bottom();
            let end = to.top();
            let mid = start.1 + 22.0_f64.min((end.1 - start.1) / 2.0);
            Route2 {
                points: vec![start, (start.0, mid), (end.0, mid), end],
                label_at: ((start.0 + end.0) / 2.0, mid),
            }
        }
        Route::Around(lane) | Route::Outer(lane) => {
            let dir = if lane >= 0 { 1.0 } else { -1.0 };
            let base = if matches!(edge.route, Route::Outer(_)) {
                if dir > 0.0 {
                    extent.1
                } else {
                    extent.0
                }
            } else if dir > 0.0 {
                (from.cx + from.hw).max(to.cx + to.hw)
            } else {
                (from.cx - from.hw).min(to.cx - to.hw)
            };
            let x = base + dir * LANE_GAP * f64::from(lane.unsigned_abs());
            let start = from.side(dir);
            let end = to.side(dir);
            Route2 {
                points: vec![start, (x, start.1), (x, end.1), end],
                label_at: (x, (start.1 + end.1) / 2.0),
            }
        }
    }
}

/// The middle of the band of heights two boxes share, when it is tall enough
/// for a straight horizontal edge.
fn overlap(a: &Placed<'_>, b: &Placed<'_>) -> Option<f64> {
    let top = (a.cy - a.hh).max(b.cy - b.hh) + 4.0;
    let bottom = (a.cy + a.hh).min(b.cy + b.hh) - 4.0;
    (bottom - top >= 6.0).then_some((top + bottom) / 2.0)
}

/// An SVG path through `points` with rounded corners.
fn rounded(points: &[(f64, f64)]) -> String {
    let mut d = format!("M {:.1} {:.1}", points[0].0, points[0].1);
    for i in 1..points.len() {
        let (x, y) = points[i];
        if i + 1 == points.len() {
            let _ = write!(d, " L {x:.1} {y:.1}");
            break;
        }
        let (px, py) = points[i - 1];
        let (nx, ny) = points[i + 1];
        let before = (x - px).hypot(y - py);
        let after = (nx - x).hypot(ny - y);
        let r = CORNER.min(before / 2.0).min(after / 2.0);
        let (ax, ay) = (x - (x - px) / before * r, y - (y - py) / before * r);
        let (bx, by) = (x + (nx - x) / after * r, y + (ny - y) / after * r);
        let _ = write!(d, " L {ax:.1} {ay:.1} Q {x:.1} {y:.1} {bx:.1} {by:.1}");
    }
    d
}

pub struct Drawn {
    pub svg: String,
    /// Each node's box in SVG coordinates: x, y, width, height.
    pub boxes: BTreeMap<&'static str, (f64, f64, f64, f64)>,
}

#[allow(clippy::too_many_lines)]
pub fn draw(chart: &Chart) -> Drawn {
    let placed = place(chart);
    let index: BTreeMap<&str, &Placed<'_>> = placed.iter().map(|p| (p.node.id, p)).collect();

    let min_x = placed
        .iter()
        .map(|p| p.cx - p.hw)
        .fold(f64::INFINITY, f64::min);
    let max_x = placed
        .iter()
        .map(|p| p.cx + p.hw)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = placed
        .iter()
        .map(|p| p.cy - p.hh)
        .fold(f64::INFINITY, f64::min);
    let max_y = placed
        .iter()
        .map(|p| p.cy + p.hh)
        .fold(f64::NEG_INFINITY, f64::max);

    let routes: Vec<(&Edge, Route2)> = chart
        .edges
        .iter()
        .filter_map(|edge| {
            let from = index.get(edge.from)?;
            let to = index.get(edge.to)?;
            Some((edge, route(from, to, edge, (min_x, max_x))))
        })
        .collect();

    // The drawing's extent: the boxes, every edge, and room for lane labels.
    let mut x0 = min_x;
    let mut x1 = max_x;
    for (_, r) in &routes {
        for &(x, _) in &r.points {
            x0 = x0.min(x - 30.0);
            x1 = x1.max(x + 30.0);
        }
    }
    let lane_top = 30.0;
    let (x0, x1) = (x0 - MARGIN, x1 + MARGIN);
    let (y0, y1) = (min_y - MARGIN - lane_top, max_y + MARGIN);
    let (width, height) = (x1 - x0, y1 - y0);

    let mut out = String::new();
    let _ = write!(
        out,
        r#"<svg class="chart" viewBox="0 0 {width:.0} {height:.0}" data-width="{width:.0}" data-height="{height:.0}" role="img" aria-label="{} flowchart" xmlns="http://www.w3.org/2000/svg">"#,
        escape(chart.title)
    );
    out.push_str("<defs>");
    for tone in ["plain", "yes", "no", "aside", "hot"] {
        let _ = write!(
            out,
            r#"<marker id="ah-{}-{tone}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" class="ah ah-{tone}"/></marker>"#,
            chart.id
        );
    }
    let _ = write!(
        out,
        "</defs><g transform=\"translate({:.1} {:.1})\">",
        -x0, -y0
    );

    // Lanes.
    let row_extent = |a: f64, b: f64| {
        let top = placed
            .iter()
            .filter(|p| (p.node.row - a).abs() < 0.51)
            .map(|p| p.cy - p.hh)
            .fold(a * ROW - CARD_H / 2.0, f64::min);
        let bottom = placed
            .iter()
            .filter(|p| (p.node.row - b).abs() < 0.51)
            .map(|p| p.cy + p.hh)
            .fold(b * ROW + CARD_H / 2.0, f64::max);
        (top - 26.0, bottom + 12.0)
    };
    match chart.lanes {
        Lanes::Rows(bands) => {
            for (i, &(label, a, b)) in bands.iter().enumerate() {
                let (top, bottom) = row_extent(a, b);
                let alt = if i % 2 == 1 { " alt" } else { "" };
                let _ = write!(
                    out,
                    r#"<rect class="lane{alt}" x="{:.1}" y="{top:.1}" width="{:.1}" height="{:.1}" rx="12"/><text class="lane-label" x="{:.1}" y="{:.1}">{}</text>"#,
                    x0 + 8.0,
                    width - 16.0,
                    bottom - top,
                    min_x + 2.0,
                    top + 16.0,
                    escape(&label.to_uppercase())
                );
            }
        }
        Lanes::Cols(bands) => {
            for (i, &(label, a, b)) in bands.iter().enumerate() {
                let left = a * COL - CARD_W / 2.0 - 22.0;
                let right = b * COL + CARD_W / 2.0 + 22.0;
                let alt = if i % 2 == 1 { " alt" } else { "" };
                let _ = write!(
                    out,
                    r#"<rect class="lane{alt}" x="{left:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="12"/><text class="lane-label" x="{:.1}" y="{:.1}">{}</text>"#,
                    y0 + 8.0,
                    right - left,
                    height - 16.0,
                    left + 14.0,
                    y0 + 24.0,
                    escape(&label.to_uppercase())
                );
            }
        }
    }

    // Edges, then their label chips over them.
    out.push_str(r#"<g class="edges">"#);
    for (edge, r) in &routes {
        let tone = tone(edge);
        let _ = write!(
            out,
            r#"<path class="edge e-{tone}" d="{}" data-from="{}" data-to="{}" data-tone="{tone}" marker-end="url(#ah-{}-{tone})"/>"#,
            rounded(&r.points),
            edge.from,
            edge.to,
            chart.id
        );
    }
    for (edge, r) in &routes {
        let Some(label) = edge.label else { continue };
        let tone = tone(edge);
        #[allow(clippy::cast_precision_loss)]
        let w = label.chars().count() as f64 * 6.3 + 14.0;
        let (lx, ly) = r.label_at;
        let _ = write!(
            out,
            r#"<g class="chip c-{tone}" data-from="{}" data-to="{}"><rect x="{:.1}" y="{:.1}" width="{w:.1}" height="18" rx="9"/><text x="{lx:.1}" y="{:.1}">{}</text></g>"#,
            edge.from,
            edge.to,
            lx - w / 2.0,
            ly - 9.0,
            ly + 4.0,
            escape(label)
        );
    }
    out.push_str("</g>");

    // Nodes.
    out.push_str(r#"<g class="nodes">"#);
    let mut boxes = BTreeMap::new();
    for p in &placed {
        let node = p.node;
        let (x, y, w, h) = (p.cx - p.hw, p.cy - p.hh, p.hw * 2.0, p.hh * 2.0);
        boxes.insert(node.id, (x - x0, y - y0, w, h));
        let code = if node.func.is_some() { " has-code" } else { "" };
        let _ = write!(
            out,
            r#"<g class="node k-{}{code}" data-id="{}" tabindex="0" role="button" aria-label="{}"><rect class="card" x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="10"/><rect class="bar" x="{:.1}" y="{:.1}" width="3.5" height="{:.1}" rx="1.75"/><g class="icon" transform="translate({:.1} {:.1})">{}</g>"#,
            kind(node.shape),
            node.id,
            escape(node.title),
            x + 1.5,
            y + 9.0,
            h - 18.0,
            x + 23.0,
            p.cy,
            icon(node.shape)
        );
        let text_x = x + 41.0;
        let rows = p.lines.len() + usize::from(!node.sub.is_empty());
        #[allow(clippy::cast_precision_loss)]
        let first = p.cy - (rows as f64 - 1.0) * 8.5 + 4.5;
        for (i, line) in p.lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let ty = first + i as f64 * LINE_H;
            let _ = write!(
                out,
                r#"<text class="title" x="{text_x:.1}" y="{ty:.1}">{}</text>"#,
                escape(line)
            );
        }
        if !node.sub.is_empty() {
            #[allow(clippy::cast_precision_loss)]
            let ty = first + p.lines.len() as f64 * LINE_H + 1.0;
            let _ = write!(
                out,
                r#"<text class="sub" x="{text_x:.1}" y="{ty:.1}">{}</text>"#,
                escape(node.sub)
            );
        }
        if node.func.is_some() {
            let _ = write!(
                out,
                r#"<g class="code-tag"><rect x="{:.1}" y="{:.1}" width="26" height="15" rx="4"/><text x="{:.1}" y="{:.1}">&lt;/&gt;</text></g>"#,
                x + w - 33.0,
                y + 7.0,
                x + w - 20.0,
                y + 18.0
            );
        }
        out.push_str("</g>");
    }
    out.push_str("</g></g></svg>");

    Drawn { svg: out, boxes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::CHARTS;

    #[test]
    fn long_titles_wrap_onto_two_balanced_lines() {
        assert_eq!(wrap("Probe out"), vec!["Probe out"]);
        assert_eq!(
            wrap("Walk the tree, scouts go home now"),
            vec!["Walk the tree,", "scouts go home now"]
        );
    }

    #[test]
    fn every_chart_draws_every_edge() {
        for chart in CHARTS {
            let drawn = draw(chart);
            assert_eq!(
                drawn.svg.matches(r#"class="edge "#).count(),
                chart.edges.len(),
                "{}",
                chart.id
            );
            assert_eq!(drawn.boxes.len(), chart.nodes.len(), "{}", chart.id);
        }
    }

    #[test]
    fn edges_join_existing_boxes_and_ids_are_unique() {
        for chart in CHARTS {
            let mut ids: Vec<&str> = chart.nodes.iter().map(|n| n.id).collect();
            ids.sort_unstable();
            let count = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), count, "duplicate node id in {}", chart.id);
            for edge in chart.edges {
                assert!(
                    ids.contains(&edge.from) && ids.contains(&edge.to),
                    "{}: {} -> {}",
                    chart.id,
                    edge.from,
                    edge.to
                );
            }
        }
    }
}
