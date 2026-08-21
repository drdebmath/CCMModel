# Simulation model

## Graph and ports

The graph is finite, undirected, and port labeled. `PortGraph` stores a dense
table for every node; a local `(node, port)` lookup returns both the neighboring
node and the reciprocal port. Construction rejects invalid nodes, missing
reciprocals, inconsistent reverse edges, and port counts outside the `u16`
domain. Node IDs and agent IDs are dense `u32` values and port IDs are `u16`.

The experiment runner currently provides paths, cycles, stars, complete
graphs, trees, grids, seeded random connected graphs, seeded random
bounded-degree graphs, and explicit port tables. Port orders can be canonical,
seeded random, adversarial (reverse canonical), or explicit.

## Placement and scheduling

Placements are dense vectors in agent-ID order. Supported policies are one
node, seeded uniform selection from a fixed-size node pool, clustered centers,
and explicit vectors.

The algorithms are deterministic centralized simulations of synchronous
logical operations; they do not create an operating-system task per agent.
Drop-and-Freeze uses explicit probe-out, probe-back, and movement barriers.
Help-by-Scouts preserves the current repository's rooted activation order and
uses explicit logical traversal counts. See [behavior-spec.md](behavior-spec.md)
for the exact compatibility semantics and discrepancies from Sudo et al.

## State and indexes

Movement uses dense agent state. Ownership and occupancy queries use node- or
agent-indexed vectors rather than repeated whole-population owner scans. Homes,
once assigned by Help-by-Scouts, are immutable; temporary scout movement does
not transfer ownership.

## Termination

Successful completion, configured limits, cancellation, invalid
configuration, and invariant violations are distinct states. A configured
limit is never labeled successful. Both algorithms return partial state and
the metrics accumulated before a limit as `round_limit_reached`.

## Observers

Metrics record algorithmic operations, not implementation loop counts or
elapsed time. Trace recorders observe the same transitions without changing
them. Trace-off equivalence tests enforce that traced and untraced execution
produce the same final state and counters.
