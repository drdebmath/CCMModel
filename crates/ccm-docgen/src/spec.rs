//! The flowcharts, as data.
//!
//! Nodes carry grid coordinates rather than being laid out automatically. A
//! solver would have to be told where to put things anyway to read well at this
//! size, and placing them by hand keeps the diagrams stable: adding a node does
//! not shuffle the rest.

/// What a box means, which decides its shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    /// Rounded pill: an entry point.
    Entry,
    /// Rectangle: a step. Usually a function.
    Step,
    /// Diamond: a branch.
    Decision,
    /// Pill: a terminal state.
    Terminal,
    /// Dashed rectangle: a helper called from the step beside it.
    Aside,
}

pub struct Node {
    pub id: &'static str,
    /// Text in the box. `|` separates lines.
    pub label: &'static str,
    pub col: f64,
    pub row: f64,
    pub shape: Shape,
    /// Function this box stands for, if clicking it should show code. The name
    /// must exist in the crate named by the chart.
    pub func: Option<&'static str>,
}

/// How an edge gets from one box to another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    /// Straight line between the nearest sides.
    Direct,
    /// Down out of the source, across, and into the top of the target.
    Elbow,
    /// Out of the source's side, around at `x`, and back into the target.
    Around(i32),
}

pub struct Edge {
    pub from: &'static str,
    pub to: &'static str,
    pub label: Option<&'static str>,
    pub route: Route,
    /// Drawn dashed: a call rather than a step in the flow.
    pub aside: bool,
}

pub struct Chart {
    pub id: &'static str,
    pub title: &'static str,
    pub crate_name: &'static str,
    pub source: &'static str,
    pub blurb: &'static str,
    pub nodes: &'static [Node],
    pub edges: &'static [Edge],
}

const fn n(
    id: &'static str,
    label: &'static str,
    col: f64,
    row: f64,
    shape: Shape,
    func: Option<&'static str>,
) -> Node {
    Node {
        id,
        label,
        col,
        row,
        shape,
        func,
    }
}

const fn e(
    from: &'static str,
    to: &'static str,
    label: Option<&'static str>,
    route: Route,
) -> Edge {
    Edge {
        from,
        to,
        label,
        route,
        aside: false,
    }
}

const fn aside(from: &'static str, to: &'static str, label: Option<&'static str>) -> Edge {
    Edge {
        from,
        to,
        label,
        route: Route::Direct,
        aside: true,
    }
}

pub static CHARTS: &[Chart] = &[
    Chart {
        id: "drop",
        title: "Drop and Freeze",
        crate_name: "ccm-algorithms",
        source: "crates/ccm-algorithms/src/lib.rs",
        blurb: "Three phases per round. Unsettled agents fan out one per port, come back \
                carrying one bit — was the neighbour occupied — and then one settles while the \
                rest move on together through a port a probe reported empty. When no empty port \
                is left the group backtracks along the port it arrived by.",
        nodes: &[
            n(
                "start",
                "simulate_with()",
                0.0,
                0.0,
                Shape::Entry,
                Some("simulate_with"),
            ),
            n(
                "canon",
                "canonicalize_ports()",
                0.0,
                1.0,
                Shape::Step,
                Some("canonicalize_ports"),
            ),
            n("settled", "all_settled()?", 0.0, 2.0, Shape::Decision, None),
            n("done", "Completed", -1.15, 2.0, Shape::Terminal, None),
            n("limit", "round < limit?", 0.0, 3.0, Shape::Decision, None),
            n(
                "capped",
                "RoundLimitReached",
                -1.15,
                3.0,
                Shape::Terminal,
                None,
            ),
            n(
                "pout",
                "probe_out()",
                0.0,
                4.0,
                Shape::Step,
                Some("probe_out"),
            ),
            n(
                "pback",
                "probe_back()",
                0.0,
                5.0,
                Shape::Step,
                Some("probe_back"),
            ),
            n(
                "mout",
                "move_out()",
                0.0,
                6.0,
                Shape::Step,
                Some("move_out"),
            ),
            n("valid", "validate_state()", 0.0, 7.0, Shape::Step, None),
            n(
                "ports",
                "ordered_ports()",
                1.2,
                4.0,
                Shape::Aside,
                Some("ordered_ports"),
            ),
            n(
                "scouts",
                "scouts_for_count()",
                1.2,
                6.0,
                Shape::Aside,
                Some("scouts_for_count"),
            ),
        ],
        edges: &[
            e("start", "canon", None, Route::Direct),
            e("canon", "settled", None, Route::Direct),
            e("settled", "done", Some("yes"), Route::Direct),
            e("settled", "limit", Some("no"), Route::Direct),
            e("limit", "capped", Some("no"), Route::Direct),
            e("limit", "pout", Some("yes"), Route::Direct),
            e("pout", "pback", None, Route::Direct),
            e("pback", "mout", None, Route::Direct),
            e("mout", "valid", None, Route::Direct),
            e("valid", "settled", Some("next round"), Route::Around(2)),
            aside("pout", "ports", Some("port order")),
            aside("mout", "scouts", Some("group size")),
        ],
    },
    Chart {
        id: "help",
        title: "Help by Scouts",
        crate_name: "ccm-help-scouts",
        source: "crates/ccm-help-scouts/src/lib.rs",
        blurb: "Also a DFS, but settled agents help. One whose node can spare it vacates and \
                travels with the group as a scout, so several ports are probed in the same round. \
                Scouts later retrace the tree in post-order and drop back onto the nodes they own.",
        nodes: &[
            n("start", "run() / simulate()", 0.0, 0.0, Shape::Entry, None),
            n(
                "trans",
                "run_transitions()",
                0.0,
                1.0,
                Shape::Step,
                Some("run_transitions"),
            ),
            n("left", "unsettled left?", 0.0, 2.0, Shape::Decision, None),
            n("settle", "settle() highest ID", 0.0, 3.0, Shape::Step, None),
            n(
                "vacate",
                "can_vacate()",
                0.0,
                4.0,
                Shape::Decision,
                Some("can_vacate"),
            ),
            n("pool", "joins scout pool", 1.25, 4.0, Shape::Aside, None),
            n(
                "probe",
                "parallel_probe()",
                0.0,
                5.0,
                Shape::Step,
                Some("parallel_probe"),
            ),
            n(
                "rank",
                "candidate_rank()",
                0.0,
                6.0,
                Shape::Decision,
                Some("candidate_rank"),
            ),
            n(
                "fwd",
                "move_group() forward",
                -0.05,
                7.0,
                Shape::Step,
                Some("move_group"),
            ),
            n(
                "back",
                "move_group() to parent",
                1.35,
                7.0,
                Shape::Step,
                None,
            ),
            n("update", "update_node_type()", 0.0, 8.0, Shape::Step, None),
            n(
                "retrace",
                "retrace()",
                -1.4,
                3.0,
                Shape::Step,
                Some("retrace"),
            ),
            n("home", "settle_vacated_at()", -1.4, 4.0, Shape::Step, None),
            n(
                "final",
                "validate_final()",
                -1.4,
                5.0,
                Shape::Terminal,
                None,
            ),
            n("etype", "edge_type()", 1.25, 5.0, Shape::Aside, None),
        ],
        edges: &[
            e("start", "trans", None, Route::Direct),
            e("trans", "left", None, Route::Direct),
            e("left", "retrace", Some("no"), Route::Direct),
            e("left", "settle", Some("yes"), Route::Direct),
            e("settle", "vacate", None, Route::Direct),
            e("vacate", "pool", Some("settledScout"), Route::Direct),
            e("vacate", "probe", Some("settled"), Route::Direct),
            e("pool", "probe", None, Route::Direct),
            e("probe", "rank", None, Route::Direct),
            e("rank", "fwd", Some("port"), Route::Direct),
            e("rank", "back", Some("none"), Route::Direct),
            e("fwd", "update", None, Route::Direct),
            e("back", "update", None, Route::Elbow),
            e("update", "left", Some("next"), Route::Around(2)),
            e("retrace", "home", None, Route::Direct),
            e("home", "final", None, Route::Direct),
            aside("probe", "etype", Some("classify")),
        ],
    },
    Chart {
        id: "p1tree",
        title: "P1Tree",
        crate_name: "ccm-p1tree",
        source: "crates/ccm-p1tree/src/lib.rs",
        blurb: "A DFS that builds a port-one tree: every vertex ends up with an incident tree \
                edge carrying port 1 at one of its ends. Two things carry the O(k) bound — the \
                search stops at the first batch that finds somewhere to go, and the traversal \
                stops at dispersion rather than finishing the tree.",
        nodes: &[
            n("start", "run() / simulate()", 0.0, 0.0, Shape::Entry, None),
            n(
                "root",
                "settle_highest_at(root)",
                0.0,
                1.0,
                Shape::Step,
                Some("settle_highest_at"),
            ),
            n(
                "trav",
                "traverse()",
                0.0,
                2.0,
                Shape::Step,
                Some("traverse"),
            ),
            n(
                "disp",
                "dispersed?",
                0.0,
                3.0,
                Shape::Decision,
                Some("note_dispersion"),
            ),
            n(
                "retrace",
                "retrace()",
                -1.5,
                3.0,
                Shape::Step,
                Some("retrace"),
            ),
            n("post", "post_order() walk", -1.5, 4.0, Shape::Step, None),
            n("done", "Completed", -1.5, 5.0, Shape::Terminal, None),
            n(
                "search",
                "neighbourhood_search()",
                0.0,
                4.0,
                Shape::Step,
                Some("neighbourhood_search"),
            ),
            n(
                "party",
                "probe_party()",
                0.0,
                5.0,
                Shape::Step,
                Some("probe_party"),
            ),
            n(
                "state",
                "state_of() / probe_detour()",
                0.0,
                6.0,
                Shape::Step,
                Some("probe_detour"),
            ),
            n(
                "choose",
                "choose_next_edge()?",
                0.0,
                7.0,
                Shape::Decision,
                Some("choose_next_edge"),
            ),
            n(
                "defer",
                "must_defer()?",
                1.35,
                8.0,
                Shape::Decision,
                Some("must_defer"),
            ),
            n(
                "await",
                "awaits_port_one()?",
                -1.5,
                8.0,
                Shape::Decision,
                Some("awaits_port_one"),
            ),
            n(
                "partial",
                "mark partiallyVisited",
                -0.15,
                9.0,
                Shape::Step,
                None,
            ),
            n("full", "mark fullyVisited", -1.5, 9.0, Shape::Step, None),
            n(
                "advance",
                "advance()",
                1.35,
                9.0,
                Shape::Step,
                Some("advance"),
            ),
            n(
                "canvac",
                "apply_can_vacate()",
                1.35,
                10.0,
                Shape::Step,
                Some("apply_can_vacate"),
            ),
            n(
                "parvac",
                "apply_parent_vacate()",
                1.35,
                11.0,
                Shape::Step,
                Some("apply_parent_vacate"),
            ),
            n(
                "backtrack",
                "backtrack()",
                -0.8,
                10.5,
                Shape::Step,
                Some("backtrack"),
            ),
        ],
        edges: &[
            e("start", "root", None, Route::Direct),
            e("root", "trav", None, Route::Direct),
            e("trav", "disp", None, Route::Direct),
            e("disp", "retrace", Some("yes"), Route::Direct),
            e("disp", "search", Some("no"), Route::Direct),
            e("retrace", "post", None, Route::Direct),
            e("post", "done", None, Route::Direct),
            e("search", "party", None, Route::Direct),
            e("party", "state", None, Route::Direct),
            e("state", "choose", None, Route::Direct),
            e("choose", "party", Some("no, ports left"), Route::Around(1)),
            e("choose", "defer", Some("found"), Route::Direct),
            e("choose", "await", Some("exhausted"), Route::Direct),
            e("defer", "partial", Some("yes"), Route::Direct),
            e("defer", "advance", Some("no"), Route::Direct),
            e("await", "partial", Some("yes"), Route::Direct),
            e("await", "full", Some("no"), Route::Direct),
            e("advance", "canvac", None, Route::Direct),
            e("canvac", "parvac", None, Route::Direct),
            e("parvac", "disp", Some("next"), Route::Around(3)),
            e("partial", "backtrack", None, Route::Direct),
            e("full", "backtrack", None, Route::Direct),
            e("backtrack", "disp", Some("popped"), Route::Around(-2)),
        ],
    },
];
