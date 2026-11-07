# RootedAsync: gap and trace note

`ccm_algorithms::rooted_async` implements `RootedAsync()`, Algorithm 5 of
Pattanayak, Kshemkalyani, Kumar, Molla and Sharma, *Optimal Dispersion Under
Asynchrony* (arXiv:2507.01298v1), with Algorithms 2, 3, 4 and 6. It is the
A2 assignment. `ccm_algorithms::p1tree` is left as it was: it remains the
centralised, synchronous construction described in [p1tree.md](../p1tree.md).

## The model as implemented

Every agent is a separate process. One activation is one
Communicate-Compute-Move cycle (Section 2):

- **Communicate**: the agent reads its own memory and the memories of the
  agents standing at its node. Nothing else is visible to it.
- **Compute**: it rewrites its own memory and at most one co-located agent's.
- **Move**: it leaves along at most one port. It is on the edge, visible at
  neither end, until its next activation lands it.

`Cx` in `rooted_async/world.rs` is the only way protocol code reaches the
world, and it enforces those three limits. A second write in one cycle, or a
write to an agent at another node, panics.

Agent memory is the paper's Table 2 plus a command counter. The simulator also
keeps each agent's position and home, which no decision reads. They are used
only to check decisions, below.

### Schedules and time

| `Schedule` | Meaning |
|---|---|
| `Synchronous` | Every agent completes one cycle per round, all reading the state the round began with. Two writes to one agent in a round are an error. |
| `Random { seed }` | One agent at a time, chosen uniformly from a seed. Cycles are atomic: nobody acts between an agent's look and its write. |
| `Starve { seed, agent, period }` | As `Random`, but one agent acts only every `period` steps: a slow scout or a slow lead. |
| `Interleaved { seed }` | The paper's ASYNC in full. Each cycle is two steps of the random order: a look, which reads the node and computes, and a later commit, which writes and leaves. Other agents act in between, so a commit can act on a stale view. A commit that would write over memory changed since its look fails the run (`WriteConflict`), because the paper's model would silently lose that change. The web page's *Asynchronous* schedule uses this one. |

Time is counted in **epochs**: the shortest intervals in which every agent
completes a cycle (Section 2). A move's cycle completes on arrival. Under
`Synchronous` an epoch is a round.

### Group coordination under asynchrony

The paper's pseudocode moves "all agents in `A_scout`" at once, which an
asynchronous system cannot do. The lead (`a_min`) posts a numbered command
(rank, probe, move, scatter). Members carry it out the next time they see it.
The lead waits at a barrier until every member is present, idle and caught up
before posting the next command. On a move it leaves last, so no member misses
the command it is following. During construction the lead is always the
lowest-ID unsettled agent, which settles last. During Retrace the role passes
to the lowest-ID vacated agent, and moves on each time that agent reaches home.

## Decisions in P1Tree that consult the ownership index

This is the first exercise of A2. Every line below lets `p1tree.rs` know
something no agent standing where the decision is made could see.

| `p1tree.rs` | What it reads globally |
|---|---|
| `state_of` (553) | Whether any node is empty, occupied or vacated, from `settled_at` |
| `neighbourhood_search` (458-549) | Every probed neighbour's owner and node type, from `settled_at` and `node_type`; the rules (R1)-(R3) are walked only to charge their cost |
| `probe_detour` (573) | The state of the port-1 neighbour, via `state_of` |
| `apply_can_vacate` (752-799) | The node's owner and type, and the port-1 neighbour's owner |
| `apply_parent_vacate` (801-833) | The parent node's owner and whether the parent is the root |
| `settle_highest_at` (722) | Every agent's position, to find the highest ID at the node |
| `move_unsettled_to` (916) | Moves every unsettled agent at once |
| `unsettled_count` (924), `note_dispersion` (437) | Global agent counts |
| `retrace` (835-881) | The whole parent table, to compute a post-order, then places scouts directly on nodes |
| `backtrack` (699), `awaits_port_one` (427), `must_defer` (608) | The head's parent and port-1 flag, from node-indexed arrays rather than the agent settled there |

In `rooted_async` none of these exist. The same facts come from agent memory
at the node, or from scouts walking.

## A hand-worked vacated probe

From Appendix B, step 6 (Figure 5, ports numbered from 1 as in the paper).
The head is at `y`. The scout `a5` probes port 2, towards `x`, which is vacated
and partiallyVisited: its agent `a3` is travelling with the group.

1. `a5` reaches `x` on port 4. Nobody is home, and 4 is not 1, so rule (R2)
   does not apply. It walks to `x`'s port-1 neighbour, `u`.
2. At `u` nobody is home either (`a5`'s own node, vacated), and it arrived on
   port 3, not 1, so (R3b) does not apply. It walks to `u`'s port-1 neighbour,
   `v`, arriving on port 1.
3. At `v`, `a6` is home. `a5` records `xi(w) = a6` and port 1, and walks back.
4. At `y` the group is checked, rule (R3c-ii). Some `c` has
   `P1Neighbor = a6` with port 1: `a5`, which owns `u`. Some `b` has
   `P1Neighbor = a5` with port 3: `a3`, which owns `x`. Both exist, so
   `psi(x) = a3` and `x` is vacated, partiallyVisited: case (α).

`the_papers_example_resolves_its_vacated_probes_by_the_named_rules` asserts this
conclusion, and steps 1 and 4, rule for rule. The run also ends with the
appendix's final placement under every schedule.

## Where the paper needed repair

Two of these were found by the simulator's own check, not by reading. Each is
commented where it is handled in `rooted_async/`.

| # | Paper | Problem | Repair |
|---|---|---|---|
| 1 | Alg. 3 | The `visited` branch always returns, so (V5) is unreachable for a freshly visited child, though Lemma 3 relies on it. | (V5) is tried whenever the node stays occupied. |
| 2 | Alg. 3, §4.1 | `P1Neighbor` and `portAtP1Neighbor`, on which (R3a) depends, are never recorded. | Recorded by the vacating errand, and by (V5) for the parent it vacates. |
| 3 | Alg. 4 l.25, 42, 44 | Matches against scouts' temporary fields, where (R3a) names the stored `P1Neighbor`. | Matches the stored fields. |
| 4 | Alg. 4 l.38-39 | (R3c-i) concludes without returning to the head. | Every scout returns. |
| 5 | Alg. 5 l.11 | "No settled agent in v" also holds at a vacated node, so reconfiguring a partiallyVisited node would settle a second agent there. | The group carries the owner it expects, from the probe result or the parent link. |
| 6 | Alg. 5 l.25 | Sets `sibling` on every arrival, erasing it on a backtrack. | Set once, on settling or reconfiguring. |
| 7 | Alg. 6 | Walking context lives in `a_min`, which changes when `a_min` gets home. | The context is handed to the next lead. |
| 8 | Alg. 5, 6 | A reconfigured node stays listed among its old parent's children, so Retrace would follow it towards its new parent. | It keeps its old parent link and is treated as a leaf when reached that way. |
| 9 | Alg. 5 l.21-24 | The O(1) probe bound depends on this shortcut for degree ≥ k−1 (Algorithm 4 alone probes every port). | Implemented: probe k−1 ports at the root, k−2 elsewhere, and scatter the unsettled agents to empty ones. |
| 10 | (R2) | **Found by the check.** "Nobody home, arrived on port 1, so empty" is wrong when the head is that node's port-1 neighbour: (V2) vacates exactly such a child, and a later probe from the head after a backtrack called it empty. | Treated as (R3) with `z` the head. The walk to `z` is the walk home, so it costs nothing. |
| 11 | (V3), (V4), (V2) | **Found by the check.** (R1)-(R3c) recognise a vacated node only if its port-1 neighbour, or that neighbour's, is occupied, and only while that stays so. (V3) and (V4) vacate without checking, and an occupied node others rely on can itself vacate later. The smallest counterexample: two mutual port-1 neighbours, both vacated by (V3), so (R3b) calls one of them empty. In 8,208 synchronous runs, 273 probes failed this way under (R3b) and 7 under (R3c-i). | (V3) and (V4) walk the chain like (V2) and vacate only with an occupied node within two port-1 hops (one for (V2), as §4.1 states it). The node relied on is marked with `vacatedNeighbor`, which now forbids vacating under every rule. |

Repair 11 vacates fewer nodes than the paper's rules, so Lemma 4's count of
vacated nodes is no longer the paper's argument. It is measured below instead.

## Verification

- **Local view against the truth.** After every probe the simulator compares
  the group's conclusion with the true owner, and every settle and every
  arrival with its record of homes. A disagreement stops the run with
  `LocalViewDisagrees`, `WrongHead`, `NodeClaimedTwice` or
  `SettledAwayFromHome`. The global record is used only here.
- **Sweep.** Every one of 8 graph families, 3 port assignments, 6 seeds,
  n ∈ {5, 6, 7, 8, 10, 12, 16} and every k from 2 to n: 8,208 runs under
  `Synchronous`, 24,624 under three random schedules, and 49,248 under six
  interleaved ones, with no disagreement and no deadlock. Reproduce with
  `cargo run --release -p ccm-experiments --example rooted_async_sweep [seed | iSEED]`.
- **Interleaving is real.** Under `Interleaved`, about 2.3 million commits
  per 8,208-run sweep act on a view that went stale between look and commit,
  in every run. None ever writes over memory someone else changed in that
  gap, so it makes no difference whether a write replaces an agent's whole
  memory or only the fields it changed.
- **Tests** in `ccm-experiments/tests/rooted_async.rs`:
  - the paper's example run, final placement and vacate count, under every
    schedule;
  - its named probe rules;
  - 432 runs showing every schedule reaches the same placement, since
    decisions depend only on what scouts find, never on timing;
  - a starved scout and a starved lead change only the time taken;
  - rejection of a scattered start;
  - a single agent.

## Validation of the paper's claims

`ccm-experiments/tests/rooted_async_validation.rs` checks each claim
directly. `scripts/validate-rooted-async.sh` runs it in release mode,
including the large cases, and prints a PASS or FAIL line per property.

1. **Dispersion.** Every run ends with every agent settled at its own home
   and no node holding more than one agent. Checked over 8 families, 3 seeds,
   k ∈ {3, 12, 24} on 24 nodes, under synchronous, random and interleaved
   schedules.
2. **Asynchrony.**
   - Every random and every interleaved schedule gives the synchronous
     placement: 8 seeds each, every family.
   - Interleaved cycles really overlap: every run has commits acting on a
     stale view, and an atomic schedule has none.
   - Holding back any one agent, scout or lead, settled or not, changes
     neither the placement nor the step bound.
   - Random schedules really interleave: more than 1.5k activations per
     epoch, against exactly k under the synchronous schedule.
3. **Time.** O(k) is checked as its parts, each bounded with a constant that
   does not depend on k:

   | Part | Bound | Observed, k up to 256 |
   |---|---|---|
   | Epochs per DFS step (finding the next node) | ≤ 40 | ≤ 26 synchronous, ≤ 13 random, ≤ 18 interleaved, the same for k = 8 and k = 256 |
   | Probe batches per node (Remark 1) | ≤ 3 | ≤ 2 |
   | Scouts at a probe (Lemma 7) | ≥ ⌈(k−2)/3⌉ | 38-99% of k |
   | DFS steps (Claim 1: each node entered at most twice) | ≤ 4k | ≤ 2.05k |
   | Retrace epochs | ≤ 10k | ≤ 6.6k |

   A complete graph, degree k − 1, never takes longer per step than a path,
   so the degree does not enter the step cost either. The same bounds hold
   under the interleaved schedule.
4. **Memory.** Theorem 2 claims `O(log(k + Δ))` bits per agent, counting what
   an agent stores from one cycle to the next (Section 2). The simulator
   charges each field of an agent's memory what its value needs: an ID
   `⌈log₂ k⌉` bits, a port `⌈log₂ Δ⌉`, a counter the bits of its current
   value, an enum its tag and payload (`rooted_async/bits.rs`). It reports the
   most any agent holds after any write.

   | Graph | k = 8 | k = 256 | × log₂(k+Δ) at 8 → 256 |
   |---|---|---|---|
   | path | 120 bits | 182 bits | 36.1 → 22.7 |
   | star | 124 | 219 | 31.7 → 24.3 |
   | complete | 121 | 211 | 31.0 → 23.5 |
   | 3-ary tree | 159 | 241 | 44.4 → 30.0 |
   | grid, 8 wide | 120 | 237 | 36.1 → 29.5 |
   | random, 2k extra edges | 164 | 302 | 43.1 → 37.4 |
   | random, dense | 164 | 365 | 43.1 → 42.1 |

   Every run stays under 48 × log₂(k+Δ), and the multiple falls as k grows:
   thirty-two times the agents costs 60 to 200 more bits. The same figures
   hold under every schedule. This is a measurement of this encoding, not of
   the paper's; a tighter one would only lower it.

The bounds are measured, not proved. A finite run can refute O(k), but cannot
establish it. To check that the script can fail, the step bound was briefly
set to 5: properties 2 and 3 failed and the script exited non-zero.

## Measurements

Epochs to dispersion, all k agents starting at node 0 of a k-node graph with
random ports. `vac/k` is the share of agents vacated by the end.

| Graph | k | Synchronous rounds | per agent | Asynchronous epochs | per agent | vac/k |
|---|---|---|---|---|---|---|
| path | 16 | 300 | 18.8 | 141 | 8.8 | 0.44 |
| path | 64 | 1,267 | 19.8 | 524 | 8.2 | 0.48 |
| path | 256 | 5,213 | 20.4 | 1,877 | 7.3 | 0.49 |
| 3-ary tree | 16 | 427 | 26.7 | 215 | 13.4 | 0.69 |
| 3-ary tree | 64 | 1,889 | 29.5 | 804 | 12.6 | 0.73 |
| 3-ary tree | 256 | 7,726 | 30.2 | 2,983 | 11.7 | 0.76 |
| grid, 8 wide | 16 | 365 | 22.8 | 184 | 11.5 | 0.56 |
| grid, 8 wide | 64 | 1,887 | 29.5 | 801 | 12.5 | 0.56 |
| grid, 8 wide | 256 | 9,209 | 36.0 | 3,506 | 13.7 | 0.64 |
| grid, 8 wide | 512 | 19,013 | 37.1 | | | 0.64 |
| random, 2k extra edges | 16 | 357 | 22.3 | 181 | 11.3 | 0.50 |
| random, 2k extra edges | 64 | 1,766 | 27.6 | 734 | 11.5 | 0.55 |
| random, 2k extra edges | 256 | 8,469 | 33.1 | 3,177 | 12.4 | 0.64 |
| random, 2k extra edges | 512 | 20,210 | 39.5 | | | 0.68 |
| complete | 16-256 | 17 | ≤ 1.1 | 7-9 | ≤ 0.6 | 0 |

- **Paths and trees** are flat per agent: linear, as Theorem 2 claims.
- **Grids** level off: 36.0, 36.8 and 37.1 rounds per agent at k = 256, 384
  and 512, which is consistent with linear time.
- **Random graphs** drift upward per agent (33.1, 38.2, 39.5 at the same k).
  The validation above explains why. The cost of one step stays constant, but
  the number of steps per agent grows: more nodes are left partiallyVisited
  and reattached later. That count is bounded by 4k (Claim 1), and observed
  at no more than 2.05k, so the drift approaches a constant rather than
  growing without limit.
- **Complete graphs** finish in constant time through the degree ≥ k−1
  shortcut.
- **Vacated share** stays at 0.44 or more, above Lemma 4's one-third, despite
  repair 11.
- **Async versus sync.** Asynchronous epochs are fewer than synchronous rounds
  because a random schedule activates many agents more than once per epoch.

These are counts from finite runs, not proofs. Wall-clock time is not reported
as a result; for planning, a synchronous run with k = 1024 on a path takes
about 5 seconds, and an interleaved one on a dense random graph about 40.
Reproduce the k ≤ 256 rows with
`cargo run --release -p ccm-experiments --example rooted_async_scaling`.

## Command line and browser (Stages 2 and 3)

- **CLI.** `ccm run` and `ccm sweep` accept `--algorithm rooted-async` and
  `--scheduler synchronous|asynchronous|interleaved|starved:AGENT:PERIOD`. Every CSV row
  carries `scheduler` and `time_unit`, so rounds and epochs are never confused.
  Any other algorithm rejects a non-synchronous scheduler. `ccm events` prints
  one run's probes (with the rule that decided each) and DFS steps, each with
  its epoch. See `experiments/README.md`.
- **Browser.** The simulator's and dashboard's algorithm menus have a
  *RootedAsync* entry. Choosing it reveals a *Schedule* menu: synchronous, or
  asynchronous, which runs the interleaved schedule. The asynchronous order is
  seeded from the page's seed field, and the counter is labelled epochs
  instead of rounds.
  Playback replays one snapshot per epoch. This was checked in headless Edge
  on the served page: both entries complete, the replay ends dispersed, and
  the page reports no errors.
- **Golden file.** The golden sweep now pins `RootedAsync` under the
  synchronous and an asynchronous schedule as well.

## Limits

- **Measured, not proved.** The time and memory bounds above are checked on
  finite runs. A run can refute them, not establish them.
- **Memory encoding.** Bits are counted for this implementation's fields. It
  carries some bookkeeping the paper does not (command numbers, the lead's
  batch counters), all of it IDs, ports or counters bounded by O(k), so it
  changes the constant, not the order.
- **Simulator cost.** The simulator finds co-located agents through per-node
  indexes, so a run costs roughly O(k²) in total, and O(k² log k) under the
  random orders, whose epochs need about k·ln k activations. Debug builds check
  every index against a full scan of the node.
