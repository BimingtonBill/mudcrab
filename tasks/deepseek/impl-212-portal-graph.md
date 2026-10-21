# impl-212 - A portal graph: every load door, where it leads, and what is reachable nearby

Owned files: crates/engine/src/portal_graph.rs (new),crates/engine/src/lib.rs,crates/engine/src/app.rs (plugin registration only),crates/engine/src/world/database.rs (only a read query if one is missing)

Acceptance: cargo test -p engine --lib,cargo clippy --workspace --all-targets -- -D warnings,cargo fmt --all -- --check

Track: portal. From the brainstorm (`local/research/brainstorm-208-architecture-performance.md` idea 1; `brainstorm-portal-summary-2026-09-25.md` item 4): the foundation for sharing one destination between the several doors of an interior (334 of 663 interiors have more than one door in) and for predictive streaming across all nearby doors.

## What to build

A `PortalGraph` resource, built once when the world DB opens, from `door_links` and the references:
- **Nodes:** spaces (a worldspace, or an interior cell).
- **Edges:** load doors (ref id, the space it stands in, its position, the destination door and space, and the `XTEL` landing).

Queries, as pure functions with unit tests:
1. The doors of a space.
2. The doors within a radius of a point in a space.
3. All doors leading into a space, which is what a shared destination instance needs.
4. Spaces reachable within N crossings of a space.
5. For a position and heading, the doors ranked by likely next use: distance, plus the angle to the heading.

Log one summary line at startup (spaces, doors, interiors with several doors) and check it against the brainstorm's numbers: 1,238 door links into interiors, 663 interiors, 334 of them with more than one door.

Nothing else uses the graph yet. Do not change streaming or the portal.

## Checks

- Unit tests for every query, on a small fixture graph.
- A run that prints the summary line: the short tour, which must still pass.
- Under `local/`, read only `local/t212*` and the research files named above.

## Report

The API; the summary line compared with the brainstorm's counts; test names; the tour's last line.
