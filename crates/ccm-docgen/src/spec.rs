//! The flowcharts, as data. Both `docs.html` and `docs/algorithm-flowcharts.md`
//! are generated from this file, so the two can no longer disagree.
//!
//! Nodes carry grid coordinates rather than being laid out automatically. A
//! solver would have to be told where to put things anyway to read well at this
//! size, and placing them by hand keeps the diagrams stable: adding a node does
//! not shuffle the rest.
//!
//! Every chart is the control flow as implemented, checked against the code it
//! names: where the implementation departs from a paper, the chart shows what
//! the code does and the notes say why. Text fields accept a little inline
//! Markdown: `code`, **bold** and [links](url).

/// What a box means, which decides its shape: an oval for a start or a finish,
/// a rectangle for a step, a diamond for a question, and a dashed box for a
/// helper called from the step beside it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    /// Where the chart starts.
    Entry,
    /// A step. Usually a function.
    Step,
    /// A question; the arrows leaving it are its answers.
    Decision,
    /// How a run ends.
    Terminal,
    /// A helper called from the step beside it.
    Aside,
}

pub struct Node {
    pub id: &'static str,
    /// What the step does, in plain words.
    pub title: &'static str,
    /// The function or expression behind it, shown under the title.
    pub sub: &'static str,
    pub col: f64,
    pub row: f64,
    pub shape: Shape,
    /// Function whose source the page shows for this box. The name must exist
    /// in the chart's source file; `name@2` picks the second definition of that
    /// name (a method sharing its name with a free function, say).
    pub func: Option<&'static str>,
    /// What happens here, for the detail panel.
    pub desc: &'static str,
}

/// How an edge gets from one box to another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    /// Straight down, straight across, or across and then down (or up) into
    /// the target: whichever the positions call for.
    Auto,
    /// Down out of the source, across, and into the top of the target.
    Elbow,
    /// Out of the source's side, along a lane just beside the two boxes, and
    /// into the target's side. Negative goes left; the size separates lanes.
    Around(i32),
    /// As `Around`, but along a lane outside the whole chart, so the loop-back
    /// crosses nothing.
    Outer(i32),
}

pub struct Edge {
    pub from: &'static str,
    pub to: &'static str,
    pub label: Option<&'static str>,
    pub route: Route,
    /// Drawn dashed: a call rather than a step in the flow.
    pub aside: bool,
}

/// Background bands that group the steps of a chart into phases.
pub enum Lanes {
    /// Horizontal bands: (label, first row, last row).
    Rows(&'static [(&'static str, f64, f64)]),
    /// Vertical bands: (label, first column, last column).
    Cols(&'static [(&'static str, f64, f64)]),
}

pub struct Chart {
    pub id: &'static str,
    pub title: &'static str,
    /// One line for the chart list.
    pub short: &'static str,
    pub crate_name: &'static str,
    pub source: &'static str,
    /// Introduction, one paragraph per entry; an entry starting with "- " is a
    /// list item.
    pub intro: &'static [&'static str],
    /// Points worth knowing that the boxes do not show.
    pub notes: &'static [&'static str],
    pub lanes: Lanes,
    pub nodes: &'static [Node],
    pub edges: &'static [Edge],
}

const fn node(
    id: &'static str,
    title: &'static str,
    sub: &'static str,
    (col, row): (f64, f64),
    shape: Shape,
    func: Option<&'static str>,
    desc: &'static str,
) -> Node {
    Node {
        id,
        title,
        sub,
        col,
        row,
        shape,
        func,
        desc,
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
        route: Route::Auto,
        aside: true,
    }
}

use Route::{Around, Auto, Elbow, Outer};
use Shape::{Aside, Decision, Entry, Step, Terminal};

pub static CHARTS: &[Chart] = &[
    // ------------------------------------------------------------------ Drop
    Chart {
        id: "drop",
        title: "Drop and Freeze (legacy baseline)",
        short: "A DFS whose tree is held by the settled agents",
        crate_name: "ccm-algorithms",
        source: "crates/ccm-algorithms/src/drop_and_freeze.rs",
        intro: &[
            "Three phases per round, over the agents standing on each node. Unsettled agents \
             fan out one per port to look at a neighbour, come back carrying one bit — does it \
             have a settled agent — and then one of them settles where nobody has while the rest \
             move on together through a port a probe reported empty. When every port has been \
             tried the group backtracks through the settled agent's parent port, so the walk is a \
             DFS whose tree is held by the settled agents.",
            "It is meant for agents that all start on **one node**: from there, with no more \
             agents than nodes, it always finishes. The code also accepts agents on several start \
             nodes (as the Python reference did), but then a group can get stuck for good — see \
             the notes.",
            "`canonicalize_ports` is a compatibility decision rather than an algorithmic one: \
             the Python reference replaced any supplied local port labels with canonical \
             sorted-neighbour ones, so this port does too. It means Drop-and-Freeze ignores the \
             port assignment the experiment asked for.",
        ],
        notes: &[
            "**Several start nodes can stall the run.** \"Empty\" is read during the probe, and \
             a start node's agent only settles later in the same round. So a group can move \
             into another group's start node just as it becomes owned. Nobody settles there for \
             this group, so it has no parent port to go back by: once that node's ports are used \
             up, the group stays for good, and the run ends at the round limit with empty nodes \
             left. Example: tree 0–1, 0–2, 1–3 with agents starting on 0, 0, 0 and 2 — two \
             agents settle, two are stuck on node 2. The Python reference behaves the same way.",
            "No empty port does not always mean backtrack: the group stays and probes again \
             next round whenever ports remain after the cursor that this round's probers \
             (counting an agent that just settled) did not cover.",
            "\"Empty\" is read during the probe. In the same round a group standing on that \
             neighbour may settle one of its agents there, so a group can move into a node that \
             has just become owned.",
            "`validate_state` runs before the first round and after each of the three phases; \
             the chart shows it once. A bad input, a failed traversal or a broken invariant \
             returns an error instead of a result.",
            "Round limit: the rounds run as `1..=round_limit`. A run that settles in exactly the \
             last round still ends Completed.",
        ],
        lanes: Lanes::Rows(&[
            ("Set up", 0.0, 1.0),
            ("Stop?", 2.0, 3.0),
            ("Probe", 4.0, 5.0),
            ("Settle and move (at each node)", 6.0, 9.0),
            ("Check", 10.0, 10.0),
        ]),
        nodes: &[
            node(
                "start",
                "Run Drop and Freeze",
                "simulate_with()",
                (0.0, 0.0),
                Entry,
                Some("simulate_with"),
                "Checks the input (every start node is in the graph, the agent count fits a \
                 u32), relabels the ports, puts every agent unsettled on its start node, then runs \
                 rounds until every agent is settled or the round limit is used up. It does not \
                 check that there are no more agents than nodes; with more, the run can only end \
                 at the round limit.",
            ),
            node(
                "canon",
                "Relabel the ports",
                "canonicalize_ports()",
                (0.0, 1.0),
                Step,
                Some("canonicalize_ports"),
                "Rebuilds the graph with canonical port numbers — each node's neighbours in \
                 sorted order — as the Python reference's `_init_ports` did. So sweeping \
                 `--ports` changes nothing for this algorithm.",
            ),
            node(
                "settled",
                "Every agent settled?",
                "all_settled()",
                (0.0, 2.0),
                Decision,
                Some("all_settled"),
                "Asked before the first round and after every round.",
            ),
            node(
                "done",
                "Completed",
                "Termination::Completed",
                (-1.2, 2.0),
                Terminal,
                None,
                "Every agent is settled, one per node.",
            ),
            node(
                "limit",
                "Rounds left?",
                "round <= round_limit",
                (0.0, 3.0),
                Decision,
                Some("simulate_with"),
                "The rounds run as `for round in 1..=round_limit`. With the limit used up and \
                 agents still unsettled, the run stops and returns its partial state.",
            ),
            node(
                "capped",
                "Round limit reached",
                "Termination::RoundLimitReached",
                (-1.2, 3.0),
                Terminal,
                None,
                "The run stopped at the round limit before every agent settled; the result \
                 holds the state at that moment.",
            ),
            node(
                "pout",
                "Probe out",
                "probe_out()",
                (0.0, 4.0),
                Step,
                Some("probe_out"),
                "At every node with unsettled agents, the agents (lowest ID first) are paired \
                 with the ports from the settled agent's cursor on (from the first port if \
                 nobody has settled here) and each walks to its neighbour. Only as many probe as there \
                 are ports left; the rest wait. A lone agent on a node nobody owns does not \
                 probe: it will settle there.",
            ),
            node(
                "ports",
                "Fixed port order",
                "ordered_ports()",
                (1.25, 4.0),
                Aside,
                Some("ordered_ports"),
                "Sorts a node's ports by a fixed key: first the ports other than 0 whose far end \
                 is port 0, then port 0, then the rest. The cursor is an index into this list, \
                 in both `probe_out` and `move_out`.",
            ),
            node(
                "pback",
                "Probe back",
                "probe_back()",
                (0.0, 5.0),
                Step,
                Some("probe_back"),
                "Each prober notes one bit — does the neighbour have a settled agent? — and \
                 walks back to the node it came from.",
            ),
            node(
                "settle",
                "Settle one where nobody has",
                "move_out()",
                (0.0, 6.0),
                Step,
                Some("move_out"),
                "At each node that has movers but no settled agent, the lowest-ID mover settles. \
                 Its parent port is the port its group last came in by, which is the way the \
                 group will later backtrack; on a start node there is none.",
            ),
            node(
                "movers",
                "Anyone left to move?",
                "!movers.is_empty()",
                (0.0, 7.0),
                Decision,
                Some("move_out"),
                "If the only agent here has just settled, this node is done for the round.",
            ),
            node(
                "found",
                "Empty port after the cursor?",
                "chosen.is_some()",
                (0.0, 8.0),
                Decision,
                Some("move_out"),
                "From this round's probe reports, the first port reported empty, in fixed port \
                 order, at or after the settled agent's cursor.",
            ),
            node(
                "move",
                "Move the rest through it",
                "move_out()",
                (0.0, 9.0),
                Step,
                Some("move_out"),
                "The remaining unsettled agents move together through that port, and the cursor \
                 moves past it.",
            ),
            node(
                "count",
                "Count the ports probed",
                "scouts_for_count()",
                (1.25, 7.0),
                Aside,
                Some("scouts_for_count"),
                "How many distinct ports this node's agents probed this round, including the \
                 agent that has just settled. It is how far the cursor moves; it is not the size \
                 of the group.",
            ),
            node(
                "more",
                "Untried ports left?",
                "next_port_to_try < ports",
                (1.25, 8.0),
                Decision,
                Some("move_out"),
                "No empty port was reported, so the cursor moves on by the number of ports just \
                 probed. If ports remain beyond it, there is still somewhere to look.",
            ),
            node(
                "wait",
                "Stay, probe more next round",
                "",
                (1.25, 9.0),
                Step,
                Some("move_out"),
                "The group does not move this round; in the next one its agents probe the ports \
                 beyond the cursor.",
            ),
            node(
                "parent",
                "Parent port?",
                "parent_port.is_some()",
                (2.5, 8.0),
                Decision,
                Some("move_out"),
                "Every port has been tried. A node where an agent settled after its group \
                 arrived has a parent port; a start node does not.",
            ),
            node(
                "back",
                "Backtrack by the parent port",
                "move_out()",
                (2.5, 9.0),
                Step,
                Some("move_out"),
                "The group leaves through the settled agent's parent port, the port its group \
                 came in by.",
            ),
            node(
                "stuck",
                "Stuck here for good",
                "",
                (3.75, 9.0),
                Step,
                Some("move_out"),
                "No port left to try and no parent to return to: the group stays, and since the \
                 cursor is at the end it never probes again. With one start node and no more \
                 agents than nodes this is never reached. It is reached when there are more agents \
                 than nodes, or with several start nodes (see the notes).",
            ),
            node(
                "valid",
                "Check invariants",
                "validate_state()",
                (0.0, 10.0),
                Step,
                Some("validate_state"),
                "Runs before the first round and after each of the three phases — it is shown \
                 once here. A broken invariant stops the run with an error.",
            ),
        ],
        edges: &[
            e("start", "canon", None, Auto),
            e("canon", "settled", None, Auto),
            e("settled", "done", Some("yes"), Auto),
            e("settled", "limit", Some("no"), Auto),
            e("limit", "capped", Some("no"), Auto),
            e("limit", "pout", Some("yes"), Auto),
            e("pout", "pback", None, Auto),
            e("pback", "settle", None, Auto),
            e("settle", "movers", None, Auto),
            e("movers", "found", Some("yes"), Auto),
            e("movers", "valid", Some("no"), Around(-1)),
            e("found", "move", Some("yes"), Auto),
            e("found", "more", Some("no"), Auto),
            e("more", "wait", Some("yes"), Auto),
            e("more", "parent", Some("no"), Auto),
            e("parent", "back", Some("yes"), Auto),
            e("parent", "stuck", Some("no"), Auto),
            e("move", "valid", None, Auto),
            e("wait", "valid", None, Elbow),
            e("back", "valid", None, Elbow),
            e("stuck", "valid", None, Elbow),
            e("valid", "settled", Some("next round"), Outer(1)),
            aside("pout", "ports", Some("port order")),
            aside("more", "count", Some("ports probed")),
        ],
    },
    // ------------------------------------------------------------------ Help
    Chart {
        id: "help",
        title: "Help by Scouts (legacy baseline)",
        short: "A DFS where settled agents help probe",
        crate_name: "ccm-algorithms",
        source: "crates/ccm-algorithms/src/help_by_scouts.rs",
        intro: &[
            "Also a DFS, but settled agents help. A settled agent whose node can spare it \
             vacates and travels with the group as a scout, so more agents share the probing \
             of a node's ports. When nobody is left unsettled, the scouts walk the tree depth \
             first and each drops back onto its own node when the group reaches it, so the run \
             ends with one settled agent on each occupied node.",
            "Like the Python reference it is rooted: placements on several start nodes are not \
             supported (a test records the reference's failure on them).",
        ],
        notes: &[
            "The order inside a step matters: `can_vacate` reads the node type that \
             `update_node_type` has just set from the probe reports.",
            "An owner that vacates joins this step's move and the next step's probe — not the \
             probe already done.",
            "Only when `can_vacate` says the owner stays can it have released the parent \
             node's owner — the root's included — into the group.",
            "A scout is not always out until the retrace: if the group comes back to its node \
             and it may no longer leave, it settles back in there.",
            "Every step spends logical rounds through `tick()`, and so does each move of the \
             retrace; the first one \
             past the limit ends the run as RoundLimitReached. Backtracking at the root is an \
             error (`MissingParent`).",
        ],
        lanes: Lanes::Cols(&[("Going home", -1.4, -1.4), ("Exploring", 0.0, 1.35)]),
        nodes: &[
            node(
                "start",
                "Run Help by Scouts",
                "run()",
                (0.0, 0.0),
                Entry,
                Some("run"),
                "Runs the main loop. If it returns normally, `validate_final` checks the result \
                 and the run is Completed; if any step went past the round limit, the run ends \
                 as RoundLimitReached with its partial state.",
            ),
            node(
                "trans",
                "Main loop",
                "run_transitions()",
                (0.0, 1.0),
                Step,
                Some("run_transitions"),
                "One DFS step at the head's node per pass, while any agent is unsettled. The \
                 head is the first active agent.",
            ),
            node(
                "capped",
                "Round limit reached",
                "Termination::RoundLimitReached",
                (1.35, 1.0),
                Terminal,
                None,
                "Every step — and each move of the retrace — spends logical rounds through \
                 `tick()`. The \
                 first one past the limit raises `RoundLimit`, which `run()` turns into this \
                 result with the state at that moment.",
            ),
            node(
                "left",
                "Any agent unsettled?",
                "unsettled.any()",
                (0.0, 2.0),
                Decision,
                Some("run_transitions"),
                "The loop condition. Once nobody is unsettled, the scouts go home.",
            ),
            node(
                "owned",
                "Head's node has an owner?",
                "physical_settler()",
                (0.0, 3.0),
                Decision,
                Some("physical_settler"),
                "A settled agent on the node, or its owner travelling as a scout, counts as the \
                 owner. Without one, an agent has to settle here first.",
            ),
            node(
                "settle",
                "Highest ID settles here",
                "settle()",
                (0.0, 4.0),
                Step,
                Some("run_transitions"),
                "The highest-ID unsettled agent on the node settles and makes it its home; its \
                 parent is the previous node's owner.",
            ),
            node(
                "more",
                "Anyone still unsettled?",
                "unsettled.any()",
                (0.0, 5.0),
                Decision,
                Some("run_transitions"),
                "If that settled the last unsettled agent, the step ends here — no probe, no \
                 move — and the scouts go home.",
            ),
            node(
                "probe",
                "Probe ports in parallel",
                "parallel_probe()",
                (0.0, 6.0),
                Step,
                Some("parallel_probe"),
                "The active agents — the unsettled ones plus the scouts — probe the node's ports \
                 except the parent port, one port per agent per batch; a batch costs two rounds. \
                 The best-ranked report gives the next port, if there is one.",
            ),
            node(
                "etype",
                "Classify the edge",
                "edge_type()",
                (1.35, 5.62),
                Aside,
                Some("edge_type"),
                "Where port 0 is: at both ends (one-one), only at the far end (other-one), only \
                 at this end (one-other), or at neither (other-other).",
            ),
            node(
                "rank",
                "Rank the reports",
                "candidate_rank()",
                (1.35, 6.38),
                Aside,
                Some("candidate_rank"),
                "Unvisited neighbours first, then partially visited ones reached on their port \
                 0; then by edge type and port. Anything else ranks 99 and is never chosen.",
            ),
            node(
                "update",
                "Update this node's type",
                "update_node_type()",
                (0.0, 7.0),
                Step,
                Some("update_node_type"),
                "From the reports: no unvisited neighbour makes it fully visited. Otherwise it is \
                 partially visited when its parent edge and every edge to an unvisited neighbour \
                 are other-other, and visited if not; the root, which has no parent edge, is \
                 then always visited.",
            ),
            node(
                "vacate",
                "Can the owner leave?",
                "can_vacate()",
                (0.0, 8.0),
                Decision,
                Some("can_vacate"),
                "Checked in this order. The root's owner: no. A visited owner, or a fully visited \
                 one nobody leans on: it steps over port 0 and back, and the answer is yes if an \
                 owner is there (that owner is marked as leaned on). A partially visited owner: \
                 yes. Otherwise no — but if this node hangs off its parent's port 0, the owner \
                 walks to the parent and releases the parent's owner (the root's too) into the \
                 group, unless someone already leans on it.",
            ),
            node(
                "pool",
                "Owner joins the scouts",
                "vacated = true",
                (1.35, 8.0),
                Step,
                Some("run_transitions"),
                "The owner travels with the group from this step's move on, and probes with it \
                 from the next step. It goes home in the retrace, or earlier if the group comes \
                 back to its node and it may no longer leave.",
            ),
            node(
                "found",
                "Did the probe find a port?",
                "next_port",
                (0.0, 9.0),
                Decision,
                Some("run_transitions"),
                "The answer `parallel_probe` gave before the node type was updated.",
            ),
            node(
                "fwd",
                "Move the group forward",
                "move_group()",
                (0.0, 10.0),
                Step,
                Some("run_transitions"),
                "All active agents, scouts included, move through the chosen port. A partially \
                 visited node entered on its port 0 is reconfigured: its parent becomes this \
                 node and it becomes visited.",
            ),
            node(
                "back",
                "Backtrack to the parent",
                "move_group(parent_port)",
                (1.35, 9.0),
                Step,
                Some("run_transitions"),
                "Nowhere new to go: the group returns through the owner's parent port. At the \
                 root there is none, which is an error.",
            ),
            node(
                "retrace",
                "Walk the tree, scouts go home",
                "retrace()",
                (-1.4, 5.0),
                Step,
                Some("retrace"),
                "A depth-first walk over the tree, starting where the scouts are. At every node \
                 it reaches, `settle_vacated_at` drops that node's owner if it is travelling; the \
                 walk stops when no scout is left.",
            ),
            node(
                "final",
                "Check: one agent per home",
                "validate_final()",
                (-1.4, 6.0),
                Step,
                Some("validate_final"),
                "Every agent must be settled on its own home, with no two agents on one node; \
                 otherwise the run returns an error.",
            ),
            node(
                "done",
                "Completed",
                "Termination::Completed",
                (-1.4, 7.0),
                Terminal,
                None,
                "Dispersed: one settled agent per occupied node.",
            ),
        ],
        edges: &[
            e("start", "trans", None, Auto),
            e("start", "capped", Some("tick() past the limit"), Auto),
            e("trans", "left", None, Auto),
            e("left", "retrace", Some("no"), Auto),
            e("left", "owned", Some("yes"), Auto),
            e("owned", "settle", Some("no"), Auto),
            e("owned", "probe", Some("yes"), Around(-1)),
            e("settle", "more", None, Auto),
            e("more", "probe", Some("yes"), Auto),
            e("more", "retrace", Some("no"), Auto),
            e("probe", "update", None, Auto),
            e("update", "vacate", None, Auto),
            e("vacate", "pool", Some("yes"), Auto),
            e("vacate", "found", Some("no"), Auto),
            e("pool", "found", None, Elbow),
            e("found", "fwd", Some("yes"), Auto),
            e("found", "back", Some("no"), Auto),
            e("fwd", "left", Some("next step"), Outer(2)),
            e("back", "left", Some("next step"), Outer(1)),
            e("retrace", "final", None, Auto),
            e("final", "done", None, Auto),
            aside("probe", "etype", Some("classify")),
            aside("probe", "rank", Some("rank")),
        ],
    },
    // ---------------------------------------------------------------- P1Tree
    Chart {
        id: "p1tree",
        title: "P1Tree",
        short: "A DFS that builds a port-one tree",
        crate_name: "ccm-algorithms",
        source: "crates/ccm-algorithms/src/p1tree.rs",
        intro: &[
            "`DFS_P1Tree` from Pattanayak, Kshemkalyani, Kumar, Molla and Sharma, *Optimal \
             Dispersion Under Asynchrony*. A DFS that builds a **port-one tree**: in a finished \
             tree every vertex has an incident tree edge carrying port 1 at one of its ends. Among the \
             ports probed so far, edges are preferred `tp1 > t11 ~ t1q > tpq`; a node that would \
             be left without a port-1 tree edge is parked as `partiallyVisited` until its port-1 \
             neighbour comes to claim it.",
            "Two details carry the `O(k)` bound, and losing either makes the cost scale with the \
             graph instead of with the agents:",
            "- `neighbourhood_search` **stops at the first batch that finds somewhere to go**. \
             Remark 1 bounds the search by the agent count, not the degree: at most `k` nodes \
             are ever occupied, so when the degree allows, an empty neighbour turns up within \
             the first `k-1` probed ports at the root and `k-2` elsewhere.",
            "- `traverse` **stops at dispersion**. Section 5: \"the process continues until no \
             unsettled agents remain\". The tree at that moment need not yet be a P1Tree — a \
             parked node may still hold a `tpq` parent edge — and that is fine, because \
             dispersion is the goal.",
        ],
        notes: &[
            "Because the search stops at the first batch that finds something, the edge \
             preference only applies among the ports probed so far: a port-1 edge on a port not \
             yet probed can lose to an earlier batch's choice.",
            "A node reached by a `tpq` edge is parked only when its next choice would be another \
             `tpq` edge or it has no candidate at all; if a port-1 edge turns up among the ports \
             probed, it advances instead.",
            "After a move, `apply_can_vacate` and `apply_parent_vacate` both look at the node \
             just left: it is the new node's parent.",
            "A leaf (no ports but the parent's) or a search with nobody to probe skips the \
             probe steps: the search returns nothing and the node goes straight to the \
             \"none, all probed\" branch.",
            "\"Every agent settled?\" is the stop rule of `simulate()` (`Stop::AtDispersion`). \
             `simulate_with_stop` with `Stop::AtFullTree` runs on instead, and its only \
             Completed exit is the root being popped (it can still stop at the round limit).",
        ],
        lanes: Lanes::Rows(&[
            ("Start", 0.0, 2.0),
            ("Each pass", 3.0, 4.0),
            ("Parallel probe", 5.0, 9.0),
            ("Choose", 10.0, 11.0),
            ("Move on or back", 12.0, 14.0),
        ]),
        nodes: &[
            node(
                "start",
                "Run P1Tree",
                "run()",
                (0.0, 0.0),
                Entry,
                Some("run"),
                "Settles the root, runs the DFS loop, and when it ends Completed walks the \
                 scouts home.",
            ),
            node(
                "root",
                "Highest ID settles at the root",
                "settle_highest_at(root)",
                (0.0, 1.0),
                Step,
                Some("settle_highest_at"),
                "Section 4: the highest-ID agent present settles; `run` then marks the root \
                 visited.",
            ),
            node(
                "trav",
                "DFS loop",
                "traverse()",
                (0.0, 2.0),
                Step,
                Some("traverse"),
                "Algorithm 2's `while` loop, with the stack held by the parent pointers of the \
                 settled agents. Each pass starts here.",
            ),
            node(
                "disp",
                "Every agent settled?",
                "note_dispersion()",
                (0.0, 3.0),
                Decision,
                Some("traverse"),
                "At the start of each pass: once no agent is unsettled, the current step is noted as \
                 the dispersion step, and under `Stop::AtDispersion` the loop ends Completed.",
            ),
            node(
                "limit",
                "Rounds left?",
                "rounds < round_limit",
                (0.0, 4.0),
                Decision,
                None,
                "Checked at the start of every pass, after the dispersion check. It counts DFS \
                 passes, not logical rounds.",
            ),
            node(
                "capped",
                "Round limit reached",
                "Termination::RoundLimitReached",
                (1.45, 4.0),
                Terminal,
                None,
                "The loop stopped at the round limit. No retrace runs, so scouts stay where \
                 they are.",
            ),
            node(
                "search",
                "Search the neighbourhood",
                "neighbourhood_search()",
                (0.0, 5.0),
                Step,
                Some("neighbourhood_search"),
                "Section 4.2's parallel probe of the head's ports, skipping the parent port, in \
                 batches. With no such port, or nobody to probe, it returns at once with nothing \
                 found.",
            ),
            node(
                "party",
                "Gather the scouts",
                "probe_party()",
                (0.0, 6.0),
                Step,
                Some("probe_party"),
                "The travelling agents — unsettled ones and scouts. With nobody travelling, the \
                 node's own settled agent probes for itself. Done once per search.",
            ),
            node(
                "out",
                "Step out to the next batch",
                "ports.chunks(scouts)",
                (0.0, 7.0),
                Step,
                Some("neighbourhood_search"),
                "The scouts take one port each and step out together; the batch is two rounds, \
                 out and back.",
            ),
            node(
                "state",
                "Read each neighbour",
                "state_of() / probe_detour()",
                (0.0, 8.0),
                Step,
                Some("probe_detour"),
                "Rules (R1)–(R3): somebody home, or arrived on port 1, answers at once; \
                 otherwise the scout walks on to the port-1 neighbour, and sometimes once more, \
                 before it can tell empty from vacated.",
            ),
            node(
                "choose",
                "Found somewhere to go?",
                "choose_next_edge()",
                (0.0, 9.0),
                Decision,
                Some("choose_next_edge"),
                "The highest-priority edge so far to a node the head may enter: an unvisited \
                 node (if anyone is left to settle there), or a partially visited one reached on \
                 its port 1. After each batch: found ends the search; nothing yet tries the next \
                 batch.",
            ),
            node(
                "defer",
                "Would this break port 1?",
                "must_defer()",
                (1.45, 10.0),
                Decision,
                Some("must_defer"),
                "Algorithm 2, line 21: the chosen edge and the parent edge are both `tpq`, and \
                 this node has no port-1 tree edge to fall back on.",
            ),
            node(
                "await",
                "Still needs a port-1 edge?",
                "awaits_port_one()",
                (-1.45, 10.0),
                Decision,
                Some("awaits_port_one"),
                "Reached through a `tpq` edge and no tree edge here carries port 1 yet. Claim 3: \
                 such a node is parked, so its port-1 neighbour can reach it later.",
            ),
            node(
                "partial",
                "Park: partially visited",
                "PartiallyVisited",
                (0.0, 11.0),
                Step,
                None,
                "Waits for its port-1 neighbour to come and reconfigure it.",
            ),
            node(
                "full",
                "Done here: fully visited",
                "FullyVisited",
                (-1.45, 11.0),
                Step,
                None,
                "No candidate edge left and a port-1 tree edge in place (the root never waits for \
                 one).",
            ),
            node(
                "advance",
                "Move to the chosen node",
                "advance()",
                (1.45, 11.0),
                Step,
                Some("advance"),
                "The unsettled agents move through the chosen port and the head follows.",
            ),
            node(
                "vacate",
                "Owner may leave as a scout",
                "apply_can_vacate()",
                (0.0, 12.0),
                Step,
                Some("apply_can_vacate"),
                "Algorithm 3 on a node just parked or finished: the root never vacates (V1), a \
                 fully visited node vacates unless a neighbour leans on it (V3), a partially \
                 visited one always (V4). A vacated owner travels as a scout and probes with \
                 the group.",
            ),
            node(
                "backtrack",
                "Back to the parent",
                "backtrack()",
                (0.0, 13.0),
                Step,
                Some("backtrack"),
                "The head and the unsettled agents return through the parent port.",
            ),
            node(
                "popped",
                "Root popped?",
                "!backtrack()",
                (0.0, 14.0),
                Decision,
                Some("backtrack"),
                "At the root there is no parent: the stack is empty and the DFS ends Completed.",
            ),
            node(
                "settle",
                "Settle there, or reconfigure",
                "settle_highest_at()",
                (1.45, 12.0),
                Step,
                Some("advance"),
                "An unvisited node gets its highest-ID agent and becomes visited. A partially \
                 visited one is reconfigured (rule D4): this port-1 edge replaces its `tpq` \
                 parent edge, and it becomes visited.",
            ),
            node(
                "parent",
                "Record the tree edge",
                "set_parent()",
                (1.45, 13.0),
                Step,
                Some("set_parent"),
                "Stores the new parent pointer and marks both ends as having a port-1 tree edge \
                 when this edge carries port 1.",
            ),
            node(
                "release",
                "Release scouts",
                "apply_*_vacate()",
                (1.45, 14.0),
                Step,
                Some("apply_parent_vacate"),
                "Both checks look at the node just left (the new node's parent). Under V2 it \
                 vacates if its port-1 neighbour is occupied; failing that, under V5, if the edge \
                 just taken leaves it by its port 1 and nothing already leans on it.",
            ),
            node(
                "retrace",
                "Walk the scouts home",
                "retrace()",
                (-1.45, 3.0),
                Step,
                Some("retrace"),
                "Section 5.1: the scouts visit the tree's nodes in post-order from the root \
                 (children by node number), one round per node, and each is dropped as its home \
                 comes up; it stops when none is left.",
            ),
            node(
                "done",
                "Completed",
                "Termination::Completed",
                (-1.45, 4.0),
                Terminal,
                None,
                "Dispersed, with every scout back on its own node (the graph is assumed connected, as in the paper).",
            ),
        ],
        edges: &[
            e("start", "root", None, Auto),
            e("root", "trav", None, Auto),
            e("trav", "disp", None, Auto),
            e("disp", "retrace", Some("yes (AtDispersion)"), Auto),
            e("disp", "limit", Some("no"), Auto),
            e("limit", "capped", Some("no"), Auto),
            e("limit", "search", Some("yes"), Auto),
            e("search", "party", None, Auto),
            e("party", "out", None, Auto),
            e("out", "state", None, Auto),
            e("state", "choose", None, Auto),
            e("choose", "out", Some("not yet, ports left"), Around(1)),
            e("choose", "defer", Some("found"), Elbow),
            e("choose", "await", Some("none, all probed"), Elbow),
            e("defer", "partial", Some("yes"), Elbow),
            e("defer", "advance", Some("no"), Auto),
            e("await", "partial", Some("yes"), Elbow),
            e("await", "full", Some("no"), Auto),
            e("partial", "vacate", None, Auto),
            e("full", "vacate", None, Elbow),
            e("vacate", "backtrack", None, Auto),
            e("backtrack", "popped", None, Auto),
            e("popped", "retrace", Some("yes"), Outer(-1)),
            e("popped", "trav", Some("no, next pass"), Outer(-2)),
            e("advance", "settle", None, Auto),
            e("settle", "parent", None, Auto),
            e("parent", "release", None, Auto),
            e("release", "trav", Some("next pass"), Outer(1)),
            e("retrace", "done", None, Auto),
        ],
    },
    // ------------------------------------------------------------ RootedDisp
    Chart {
        id: "rooteddisp",
        title: "RootedDisp",
        short: "The rooted HEO-DFS: probers double each pass",
        crate_name: "ccm-algorithms",
        source: "crates/ccm-algorithms/src/rooted_disp.rs",
        intro: &[
            "`RootedDisp` from Sudo, Shibata, Nakamura, Kim and Masuzawa, *Near-linear Time \
             Dispersion of Mobile Agents*, Section 3 and Algorithm 1: the rooted HEO-DFS. The \
             agent with the largest ID leads a depth-first search, and the smallest-ID explorer \
             settles at each node it reaches. Every box that moves agents is exactly one \
             synchronous step, committed through `apply_move_intents`; settling takes none. \
             [rooted-disp.md](rooted-disp.md) has the time model, the decisions taken where \
             Algorithm 1 leaves a choice, and two worked traces.",
            "What makes the search fast is `probe_pass`. Every agent at the node except its \
             settler takes one port; a settler found at home comes back as a **helper** and \
             probes in the next pass too, so the probers double until one finds an unsettled \
             neighbour: `O(log τ)` passes per probe and `O(k log τ)` steps in all. `probe` \
             resets `next` and `checked` on every call, so no cursor survives from an earlier \
             visit.",
        ],
        notes: &[
            "Rooted only: every agent must start on one node. Any other placement is refused \
             with `NonRootedStart` before the first step.",
            "Two choices Algorithm 1 leaves open (rooted-disp.md, decisions 1 and 2): the \
             helpers walk home in a step of their own, taken only when there are helpers; and \
             probers take ports in ascending ID, the smallest unsettled port winning.",
            "A retreat at the root would mean the root's component has fewer nodes than agents; \
             the run returns `RootExhausted` instead of looping.",
            "Round limit: `commit` checks it before every step, so a run can stop between the \
             passes of a probe and returns its partial state as RoundLimitReached.",
        ],
        lanes: Lanes::Rows(&[
            ("Start", 0.0, 1.0),
            ("Each move", 2.0, 3.0),
            ("Probe()", 4.0, 7.0),
            ("Move on or back", 8.0, 9.0),
        ]),
        nodes: &[
            node(
                "start",
                "Run RootedDisp",
                "simulate()",
                (0.0, 0.0),
                Entry,
                Some("simulate"),
                "Checks the input (a non-empty graph, no more agents than nodes, every agent on \
                 the same start node) and builds the agent store.",
            ),
            node(
                "root",
                "Smallest ID settles at the root",
                "run(), line 1",
                (0.0, 1.0),
                Step,
                Some("run"),
                "The first explorer, the smallest ID, becomes the root's settler with no parent. \
                 It takes no step.",
            ),
            node(
                "loop",
                "Search until the leader settles",
                "disperse()",
                (0.0, 2.0),
                Step,
                Some("disperse"),
                "Lines 2–11: probe, then move forward or back, until `a_max` (the largest ID) \
                 has settled.",
            ),
            node(
                "leader",
                "Leader settled?",
                "a_max.settled",
                (0.0, 3.0),
                Decision,
                None,
                "The leader settles last, when it is the only explorer left, so this is also \
                 \"every agent settled\".",
            ),
            node(
                "done",
                "Completed",
                "Termination::Completed",
                (-1.5, 3.0),
                Terminal,
                None,
                "Every agent settled on its own node; `validate_final` confirms it.",
            ),
            node(
                "probe",
                "Probe the current node",
                "probe(), lines 12–14",
                (0.0, 4.0),
                Step,
                Some("probe"),
                "Resets the settler's `next` and `checked`, then runs passes until a port is \
                 found or every port has been checked.",
            ),
            node(
                "pass",
                "One pass: out and back",
                "probe_pass(), lines 16–21",
                (0.0, 5.0),
                Step,
                Some("probe_pass"),
                "Every agent at the node but its settler takes one of the next ports, in \
                 ascending ID. Two steps: out, then back, each bringing the settler it found. An \
                 agent that found nobody reports its port as unsettled.",
            ),
            node(
                "commit",
                "One synchronous step",
                "commit()",
                (-1.3, 5.0),
                Aside,
                Some("commit"),
                "Validates and applies every move of the step together, after checking the \
                 round limit.",
            ),
            node(
                "found",
                "Unsettled port, or all checked?",
                "next / checked",
                (0.0, 6.0),
                Decision,
                None,
                "With neither, every prober brought a helper back, so twice as many agents probe \
                 the next ports.",
            ),
            node(
                "home",
                "Helpers walk home",
                "helpers_home(), line 22",
                (0.0, 7.0),
                Step,
                Some("helpers_home"),
                "Each helper leaves through the port it arrived by, which leads home. One step, \
                 taken only when there are helpers.",
            ),
            node(
                "next",
                "Unsettled neighbour?",
                "ψ(w).next",
                (0.0, 8.0),
                Decision,
                None,
                "`next` holds the smallest port to an unsettled neighbour, or nothing when every \
                 neighbour is settled.",
            ),
            node(
                "fwd",
                "Move forward; smallest settles",
                "forward(), lines 6–9",
                (-0.8, 9.0),
                Step,
                Some("forward"),
                "All explorers move through `next` in one step; the smallest ID among them \
                 settles there with `parent` set to the leader's incoming port.",
            ),
            node(
                "bwd",
                "Move back to the parent",
                "backward(), line 11",
                (0.8, 9.0),
                Step,
                Some("backward"),
                "All explorers move through the settler's `parent` port in one step.",
            ),
        ],
        edges: &[
            e("start", "root", None, Auto),
            e("root", "loop", None, Auto),
            e("loop", "leader", None, Auto),
            e("leader", "done", Some("yes"), Auto),
            e("leader", "probe", Some("no"), Auto),
            e("probe", "pass", None, Auto),
            e("pass", "found", None, Auto),
            e("found", "pass", Some("no: twice the probers"), Around(1)),
            e("found", "home", Some("yes"), Auto),
            e("home", "next", None, Auto),
            e("next", "fwd", Some("yes"), Auto),
            e("next", "bwd", Some("no"), Auto),
            e("fwd", "leader", Some("next step"), Around(-2)),
            e("bwd", "probe", Some("probe the parent"), Around(2)),
            aside("pass", "commit", Some("each step")),
        ],
    },
];
