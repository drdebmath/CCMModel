# P1Tree dispersion

`ccm-p1tree` implements the port-one tree construction of Pattanayak,
Kshemkalyani, Kumar, Molla and Sharma, *Optimal Dispersion Under Asynchrony*
([arXiv:2507.01298](https://arxiv.org/abs/2507.01298)). Unlike the other two
algorithms in this repository, it is written from the paper rather than from a
prior implementation in this tree.

## What a P1Tree is

Definition 1 of the paper. Classify each edge by the pair of local port numbers
at its two ends. Writing the port at the near end first:

| Type | Near port | Far port |
| --- | --- | --- |
| `tp1` | not 1 | 1 |
| `t11` | 1 | 1 |
| `t1q` | 1 | not 1 |
| `tpq` | not 1 | not 1 |

`tpq` is the only type with port 1 at neither end, so "carries port 1 at one of
its ends" is the same as "is not `tpq`", and the property is symmetric: `tp1`
seen from one end is `t1q` seen from the other.

A spanning tree is a **P1Tree** when every vertex has at least one incident
*tree* edge that is not `tpq`. Observation 1 of the paper guarantees this is
always achievable: every node has a `t11` or `t1q` edge, because every node has
a port 1 and it leads somewhere.

### Port numbering

The paper numbers ports from 1. This repository numbers them from 0, so the
paper's port 1 is `PortId(0)` here, exposed as `ccm_p1tree::PORT_ONE`. The UI
labels ports 0-based for the same reason, and its glossary says so.

## The construction

Algorithm 2, `DFS_P1Tree()`. A DFS with one addition. Edges are considered in
the priority order

```
tp1  >  t11 ~ t1q  >  tpq
```

with ties broken by the smaller local port. `t11` and `t1q` share a rank without
ambiguity, since both require port 1 at the near end and a node has only one
port 1.

The addition is the `partiallyVisited` node type. A node whose parent edge is
`tpq` has no claim to the Definition 1 property through that edge, so it is not
allowed to keep it: it is parked as `partiallyVisited` and the DFS backtracks
(rule D3). Rule D4 then makes that node look *empty* again, but only to its
port-1 neighbour. When the DFS eventually arrives from that neighbour, the
`tpq` parent edge is swapped for the port-1 edge and the node becomes ordinary.
The paper calls this reconfiguration; Claim 2 shows the swap creates no cycle.

### One place the pseudocode needs reading alongside the proof

Algorithm 2 marks `partiallyVisited` at line 22, which is reached only when the
node has some candidate edge to leave by. Line 29 marks a node with no candidate
edge `fullyVisited` instead. Taken literally, a node that is discovered through a
`tpq` edge and happens to have no empty neighbours would be finished off at line
29 while holding only a `tpq` tree edge, which violates Definition 1. `K4` with
canonical ports does exactly this.

Claim 3 of the proof resolves it: a vertex first discovered through a `tpq` edge
"is immediately marked partiallyVisited and DFS backtracks", so that its port-1
neighbour can reach it later. This implementation therefore only lets a node
become `fullyVisited` once it holds a port-1 incident tree edge, and the
regression test `definition_one_needs_the_walk_back_to_the_root` pins the `K4`
case.

## Executing it with agents

Section 4 of the paper. Nodes are anonymous, so the agent settled at a node *is*
that node's memory: it holds the node type and the parent pointers, and the
paper writes it `psi(x)`.

- All agents start at the root. The highest-ID agent present settles.
- **Parallel probe** (Section 4.2). The scouts fan out over the head's ports
  together: ports are handed out in increasing agent-ID order, every scout steps
  onto a different neighbour in the same round, and they all return in the next
  one, each reporting `<port, edge type, node type, psi>`. The parent port is
  skipped: the parent is known to be occupied and is reached by backtracking.
- **The search stops as soon as it finds somewhere to go.** Remark 1 bounds it
  at "k-1 ports at the root node and k-2 ports at a non-root node" — by the
  agent count, not the degree — because at most `k` nodes are ever occupied, so
  while any agent is unsettled some probed neighbour must be empty. Only when a
  batch finds nothing does the search go on to the remaining ports, which is
  exactly the case that has to rule out an empty neighbour before a node can be
  called finished.
- On reaching an unvisited node the highest-ID unsettled agent settles there,
  which is the dispersion.
- When no candidate edge is left the DFS backtracks along the parent port.

## Where it stops

Section 5: "The process continues until no unsettled agents remain." Once the
tree has `k` vertices the construction is done and Retrace follows. That is
[`Stop::AtDispersion`], and it is what [`run`] and [`simulate`] do.

**The tree at that moment is not promised to be a P1Tree.** A node parked as
`partiallyVisited` may still hold a `tpq` parent edge, waiting for a port-1
neighbour the run no longer has any reason to visit. That is fine: dispersion is
the goal, and the reconfiguration only has to happen if the construction
continues. Definition 1 and Theorem 1 describe a completed `DFS_P1Tree` run.

[`Stop::AtFullTree`], reached through [`run_to_full_tree`], keeps going to
Algorithm 2's own termination — the root popped, every node `fullyVisited` — so
the result is a real P1Tree. It exists so the property can be tested and studied.
It is not the dispersion algorithm and it costs more, sharply so when the degree
far exceeds the agent count, because the walk continues with no unsettled agents
left to make a neighbourhood search cheap.

`P1Result::dispersed_at_step` records the moment the last agent settled under
either rule.

### Why the bound is O(k) and not O(n)

Both halves matter, and getting either wrong costs the bound. Scanning every
port instead of stopping at the first find made cost scale with degree; walking
on past dispersion made it scale with `n`. With 40 agents on a 4000-node
complete graph that was 32,357 rounds against 155 DFS visits. Corrected, the
same case is 267 rounds, and identical at n = 1000, 2000 and 4000. Across
`k = 10 … 160` on `K4000`, rounds per agent stay flat at about 6.7.

### Vacating

Section 4.1, Algorithm 3 `Can_Vacate()`. A node holds only one settled agent, so
without releasing some of them the head runs out of scouts the moment the last
agent settles and the parallel probe has nobody to probe with. Vacating is the
supply: a **vacated** node keeps its owner but that owner travels with the head
as a `settledScout`.

- (V1) the root is never vacated.
- (V2) a `visited` node vacates when its port-1 neighbour is occupied. The head
  steps to that neighbour to mark that this node leaned on it, then returns.
- (V3) a `fullyVisited` node vacates unless a neighbour already leans on it.
- (V4) a `partiallyVisited` node always vacates.
- (V5) otherwise, if this node's port at its parent is port 1, the parent may
  vacate instead, provided nothing leans on it.

Lemma 4 shows at least a third of the tree is vacated at any moment, which is
what keeps the probe parallel. The effect is easy to see in the counters: on
`K12` with 12 agents, probe rounds fall from 614 without vacating to 86 with it,
and on a 12-node star from 98 to 24.

### Retrace

Section 5.1. When the construction ends the scouts are all standing wherever the
head finished, so a post-order walk of the tree puts each one back on the node it
owns. `retrace_puts_every_scout_back_on_its_own_node` checks that the run really
does end with one settled agent standing on each node, not merely owning it from
a distance.

### Telling empty from vacated

The paper's probe rules (R1)-(R3) are mostly concerned with one ambiguity: a
scout arriving at a node with no agent present cannot immediately tell whether
that node is **empty** or merely **vacated**, its settled agent having left to
scout elsewhere. The rules resolve it by walking to the port-1 neighbour and
checking transitively for the agent that owns it.

The scout's *conclusion* is taken from the simulation's ownership index rather
than re-derived through the rules, because Lemma 5 proves the rules compute
exactly that. The extra traversals the rules require are still walked and
charged, so the movement counters stay faithful: no detour under (R1) or (R2),
one hop under (R3a)/(R3b), two under (R3c).

## What is not implemented

The construction, the parallel probe, vacating and retrace are all implemented.
What is not:

**Asynchrony.** This repository's scheduler is synchronous, so the `O(k)`
*epoch* bound of `RootedAsync()` is not literally what these round counters
measure, even though they do now scale with `k` rather than with `n` or the
degree.

The tree produced is the same. The round counts are not the paper's bound, so
they are not evidence about it. `docs/complexity-metrics.md` applies as usual:
wall-clock time is never a complexity result, and the CSV counters are the
measurements to quote.

General dispersion from a scattered initial placement (Section 6) is also not
implemented. A non-rooted placement is rejected with `P1Error::NonRootedStart`
rather than silently run under a different model.

## Tests

`crates/ccm-p1tree/src/lib.rs` covers edge classification and the priority
order; dispersion across path, cycle, star, complete, tree and grid; Definition
1 and the absence of leftover `partiallyVisited` nodes; the spanning-tree shape;
`k < n`; determinism; round limits; and that tracing does not change the result.

The property test `random_port_labellings_always_yield_a_spanning_p1tree`
generates 60 connected graphs with randomly permuted port labels and checks all
of it at once. Random labellings matter here more than anywhere else in the
repository, because the algorithm is entirely about which edges happen to carry
port 1.
