# RootedDisp dispersion

`ccm_algorithms::rooted_disp` implements `RootedDisp`, the rooted HEO-DFS of
Sudo, Shibata, Nakamura, Kim and Masuzawa, *Near-linear Time Dispersion of
Mobile Agents* ([arXiv:2310.04376v3](https://arxiv.org/abs/2310.04376v3)),
Section 3 and Algorithm 1. Like P1Tree, and unlike Drop-and-Freeze and
Help-by-Scouts, it is written from the paper rather than from earlier code in
this repository.

> **Status.** This document fixes the semantics before any code is written: the
> model, the time steps, the agreed decisions and two worked traces. The module,
> its CLI registration and its browser support follow on the
> `feature/rooted-heo-dfs` branch, and the sections below that name code
> describe what those commits will contain.

## The model

Section 2 of the paper, which the implementation follows exactly:

- The graph is simple, connected, undirected and anonymous. Each node `v`
  labels its edges with local ports `0 … δ_v − 1`, independently of its
  neighbours. The paper already numbers ports from 0, so unlike P1Tree there is
  no offset between the paper and `PortId`.
- The `k ≤ n` agents have unique IDs and no global knowledge. Every agent is
  always at a node; a move along an edge is atomic.
- Time is synchronous. In one **step** every agent sees the ID, state and
  incoming port of each agent on its node, updates its own memory, and then
  either stays or moves along one edge.
- An agent's incoming port `pin` is the port at its current node through which
  it arrived in the previous step, and ⊥ if it did not move in that step.
- Agents communicate only with agents on the same node. Nodes have no memory.
- Dispersion (Definition 1) is reached when every agent is on its own node and
  no agent moves again. Time is the number of steps until then.

## Roles and memory

Every agent starts as an **explorer** (`settled = ⊥`). The agent with the
largest ID, `a_max`, is the leader of the depth-first search. An explorer that
settles becomes a **settler** (`settled = ⊤`) for good, and the node it settles
on is its **home** for good. A settler may leave home only as a **helper**:
brought to an adjacent probe centre during a `Probe()` and returned home at its
end.

The settler `ψ(w)` of a node `w` is that node's memory. Lemma 2 lists what any
agent keeps, `O(log(k + Δ))` bits in all:

| Variable | Held by | Meaning |
| --- | --- | --- |
| `settled` | every agent | explorer (⊥) or settler (⊤) |
| `parent` | `ψ(u)` | port at `u` leading to `u`'s DFS parent; ⊥ at the root |
| `next` | `ψ(w)` | port at `w` leading to an unsettled neighbour found by the last `Probe()`, or ⊥ |
| `checked` | `ψ(w)` | ports `0 … checked` of `w` are known to lead to settled nodes; starts at −1 |
| return port | a helper | port at the probe centre leading back to its home |

The implementation keeps these per agent in a `RootedMemory`, with `checked`
stored as `Option<PortId>` where `None` stands for −1.

## One run

Line numbers are those printed in Algorithm 1. The prose of Section 3 cites
lines one higher than the printed pseudocode (for example "lines 13–23" for
`Probe()`, which the figure numbers 12–22); this document uses the printed
numbers throughout.

1. All agents start at the root `s`. The smallest-ID agent settles there with
   `parent = ⊥`.
2. While `a_max` is unsettled (line 2), `a_max` runs `Probe()` at its current
   node `w` (line 3).
3. `Probe()` (lines 12–22) resets `next = ⊥` and `checked = −1` (line 14), then
   loops until `checked = δ_w − 1` (line 15):
   - every agent at `w` other than `ψ(w)` — the explorers and any helpers
     already recruited — is a candidate prober. The first `Δ′ = min(x, δ_w − 1 −
     checked)` of them each visit one port, `checked + 1 … checked + Δ′`
     (line 16);
   - a prober that finds the neighbour's settler at home brings it back to `w`
     as a helper (line 17). A prober that finds nobody has found an unsettled
     node: `next` takes its port and the loop ends (lines 18–19);
   - if every prober brought a settler back, `checked` advances by `Δ′` and the
     next pass has twice as many probers (line 21).
4. All helpers go home (line 22).
5. If `next ≠ ⊥`, every explorer moves to `u = N(w, next)` and the smallest ID
   among them settles at `u`, with `parent ← a_max.pin` (lines 5–9). Otherwise
   every explorer moves back to `N(w, ψ(w).parent)` (line 11).
6. The run ends when `a_max` settles, which happens only on a forward move that
   it makes alone.

## Time model

| Step | Line | Who moves | Counted in |
| --- | --- | --- | --- |
| probe out | 16 | the first `Δ′` agents at `w` other than `ψ(w)`, one port each | `probe_rounds` |
| probe back | 17 | every prober returns to `w`; a settler it found comes with it | `probe_rounds` |
| helpers home | 22 | every helper, through its return port | `movement_rounds` |
| forward | 7–9 | every explorer, from `w` to `u` | `movement_rounds` |
| backward | 11 | every explorer, to `N(w, ψ(w).parent)` | `movement_rounds` |

Each step is exactly one logical round. Settling and the updates to `next`,
`checked` and `parent` are local computation and take no step. Every counted
step moves at least one agent.

### Agreed decisions

Agreed on 13 October 2026.

| # | Decision | Agreed choice |
| --- | --- | --- |
| 1 | The helpers' trip home (line 22) | Its own step, charged only when at least one helper is at `w`. It is not merged with the DFS move that follows, as in the last panel of Figure 1. |
| 2 | Tie-breaks | Probers are taken in ascending agent ID. When several probers of one pass find unsettled nodes, `next` is the smallest of their ports; the paper says only "the port used by one of these agents". |
| 3 | Kernel | Physical state lives in `ccm_core::AgentStore` (positions, `pin`, permanent homes) and every step is committed through `ccm_core::apply_move_intents`. |
| 4 | CSV schema | Unchanged. Paper-specific counts appear only on the typed result. |

Decision 1 fixes the exact counts. On the two worked examples below, the three
readings of line 22 give:

| Reading of line 22 | Star, 9 nodes, 9 agents | Path, 4 nodes, 4 agents |
| --- | --- | --- |
| Own step, only when a helper exists (agreed) | 73 steps | 13 steps |
| Own step on every `Probe()` | 74 steps | 14 steps |
| Same step as the following DFS move | 59 steps | 11 steps |

All three stay within Theorem 1's `O(k log τ)`, `τ = min(k, Δ)`.

## Settler homes, helping and return paths

- **Homes are permanent.** A home is claimed once, by `AgentStore::mark_settled`;
  a second home for the same agent, or a second owner for the same node, is an
  error.
- **Helping is local.** A prober learns whether its neighbour is settled only by
  looking at the agents on that neighbour while it stands there. No settler is
  ever away from home when a prober arrives: helpers from the current `Probe()`
  live on ports already checked, and every earlier `Probe()` sent its helpers
  home before the DFS moved. The implementation checks this as an invariant.
- **A recruited helper is vacated, not moved home by fiat.** It leaves home
  through `AgentStore::vacate_home` as it rides back with its prober, and on
  arrival records its `pin` at `w` as its return port. It keeps that port while
  probing in later passes, walks home through it at line 22, and only then is
  restored by `AgentStore::return_home`.
- **The explorer group backtracks through `ψ(w).parent`**, written from
  `a_max.pin` when `ψ(w)` settled. A prober returns through its own `pin`.

## Termination

- **Completed.** Exactly when `a_max` settles (line 2). At that moment all `k`
  agents are on distinct homes, no helper is away, and no agent moves again.
- **Round limit.** The limit counts steps. Reaching it returns the partial state
  and counters as `RoundLimitReached`, never as completion.
- **Rejected input.** A placement that is not a single node, zero agents, more
  agents than nodes, or a start outside the graph is an error, as for P1Tree. A
  root with no unsettled neighbour while explorers remain — possible only when
  the root's component has fewer than `k` nodes — is an error, not completion.

Appendix A's termination-detection protocol, in which agents learn that the run
has ended, is not implemented: Algorithm 1 reaches dispersion without it.

## Counters

| Counter | Meaning for RootedDisp |
| --- | --- |
| `rounds` | time steps |
| `probe_rounds` | probe-out plus probe-back steps, two per pass |
| `movement_rounds` | forward, backward and helpers-home steps |
| `agent_moves`, `edge_traversals` | every agent crossing: probers, helpers both ways, the explorer group |
| `port_probes` | probes sent, the sum of `Δ′` over all passes |
| `probe_out_traversals`, `probe_back_traversals` | one each per probe; a helper's ride is a move, not a probe |
| `settlements` | `k` |
| `backtracks` | backward moves |
| `group_moves`, `maximum_group_size` | forward and backward moves of the explorer group |
| `maximum_tree_depth` | depth of the DFS tree |
| `peak_logical_memory_words` | Lemma 2's persistent words per agent (ID, `settled`, `parent`, `next`, `checked`, return port) times `k` |

`scout_operations`, `vacates`, `chase_operations`, `follow_operations` and
`retraces` belong to other algorithms and stay 0. The typed result adds the
paper's own counts: `Probe()` calls, passes, the most passes in one `Probe()`,
helper recruitments, and forward and backward moves.

## Worked examples

Both examples put every agent on node 0 with canonical ports: port `i` of a node
leads to its `i`-th smallest neighbour. Agent IDs are `0 … k − 1`, so `a_max` is
agent `k − 1`. These are the two hand traces of the task; they become the first
test fixtures.

### Path 0–1–2–3, four agents

| Step | Phase | Moves (agent via port) | Outcome |
| ---: | --- | --- | --- |
| 0 | start | — | agent 0 settles at node 0; `a_max` is agent 3 |
| 1 | probe out | 1 via 0 | `Probe()` at node 0, pass 1 |
| 2 | probe back | 1 | node 1 is unsettled, so `next = 0` |
| 3 | forward | 1, 2, 3 via 0 | agent 1 settles at node 1, `parent = 0` |
| 4 | probe out | 2 via 0, 3 via 1 | `Probe()` at node 1, pass 1 |
| 5 | probe back | 2 with settler 0, 3 | node 2 is unsettled, so `next = 1`; agent 0 is a helper |
| 6 | helpers home | 0 via 0 | |
| 7 | forward | 2, 3 via 1 | agent 2 settles at node 2, `parent = 0` |
| 8 | probe out | 3 via 0 | `Probe()` at node 2, pass 1 |
| 9 | probe back | 3 with settler 1 | node 1 is settled, `checked = 0`; agent 1 is a helper |
| 10 | probe out | 1 via 1 | pass 2: the helper, the smallest ID at node 2, probes |
| 11 | probe back | 1 | node 3 is unsettled, so `next = 1` |
| 12 | helpers home | 1 via 0 | |
| 13 | forward | 3 via 1 | `a_max` settles alone at node 3: completed |

13 steps, 3 `Probe()` calls, no backtrack, 5 probes, 2 helper recruitments and
20 agent moves. In step 10 the node that ends the run is discovered by a
recruited helper, not by an explorer.

### Star with hub 0 and leaves 1–8, nine agents

Port `i − 1` of the hub leads to leaf `i`, and each leaf's only port leads back
to the hub. One row per `Probe()`:

| Probe | At node | Agents at `w` but `ψ(w)`, per pass | Outcome | Then | Steps so far |
| ---: | ---: | --- | --- | --- | ---: |
| 1 | 0 | 8 | port 0 unsettled; no helper | forward to leaf 1, agent 1 settles | 3 |
| 2 | 1 | 7 | the hub is settled; agent 0 helps | backward | 7 |
| 3 | 0 | 7 | port 0 settled, port 1 unsettled; 1 helper | forward to leaf 2, agent 2 settles | 11 |
| 4 | 2 | 6 | the hub is settled; agent 0 helps | backward | 15 |
| 5 | 0 | 6 | ports 0–1 settled, port 2 unsettled; 2 helpers | forward to leaf 3, agent 3 settles | 19 |
| 6 | 3 | 5 | the hub is settled; agent 0 helps | backward | 23 |
| 7 | 0 | 5 | ports 0–2 settled, port 3 unsettled; 3 helpers | forward to leaf 4, agent 4 settles | 27 |
| 8 | 4 | 4 | the hub is settled; agent 0 helps | backward | 31 |
| 9 | 0 | 4 → 8 | ports 0–3 settled, then ports 4–7 all unsettled, so `next = 4`; 4 helpers | forward to leaf 5, agent 5 settles | 37 |
| 10 | 5 | 3 | the hub is settled; agent 0 helps | backward | 41 |
| 11 | 0 | 3 → 6 | ports 0–4 settled, port 5 unsettled; 5 helpers | forward to leaf 6, agent 6 settles | 47 |
| 12 | 6 | 2 | the hub is settled; agent 0 helps | backward | 51 |
| 13 | 0 | 2 → 4 → 8 | ports 0–5 settled, then ports 6–7 unsettled, so `next = 6`; 6 helpers | forward to leaf 7, agent 7 settles | 59 |
| 14 | 7 | 1 | the hub is settled; agent 0 helps | backward | 63 |
| 15 | 0 | 1 → 2 → 4 → 8 | ports 0–6 settled, port 7 unsettled; 7 helpers | forward to leaf 8, `a_max` settles: completed | 73 |

73 steps: 15 `Probe()` calls with 22 passes (44 probe steps), 14 helpers-home
steps, 8 forward and 7 backward moves. There are 65 probes, 35 helper
recruitments and 264 agent moves.

Probe 13 is Figure 1 of the paper: two agents at the hub besides `ψ(0)`, then
four, then eight, after which the last two ports are probed and an unsettled
leaf is found. Probe 15 needs four passes, `⌊log₂(τ + 1)⌋ + 1 = 4` with
`τ = 8`, which is the most any `Probe()` can need here. Probes 9 and 13 exercise
decision 2: several probers find unsettled leaves in the same pass, and the
smallest port wins.

## What it does not take from the legacy baselines

Drop-and-Freeze and Help-by-Scouts are not ports of this paper (see
[behavior-spec.md](behavior-spec.md)), and P1Tree takes two simulation
shortcuts. RootedDisp avoids all of them:

| Shortcut | Found in | RootedDisp instead |
| --- | --- | --- |
| Supplied port labels replaced by canonical ones | Drop-and-Freeze | uses the port labels as given |
| A probe cursor kept between calls | Drop-and-Freeze | resets `next` and `checked` on every `Probe()` |
| Settlers never recruited | Drop-and-Freeze | found settlers come back and probe |
| Neighbour status read from a global ownership index | Help-by-Scouts, P1Tree | observed by the prober standing on the neighbour |
| Every port probed, no doubling | Help-by-Scouts | stops at the first unsettled neighbour |
| The highest ID settles | Help-by-Scouts | the smallest ID settles; the largest leads |
| Rounds charged when nobody moves | Help-by-Scouts | one round per step, and every step moves an agent |
| A final walk that jumps between non-adjacent nodes | P1Tree | each helper goes home over one real edge |

## What is not implemented

- `GeneralDisp` (Section 4), for agents starting on several nodes.
- `RootedOpt` (Algorithm 6), the paper's other rooted algorithm.
- Appendix A's termination detection.

A placement that is not rooted is rejected rather than run under a different
model.
