# Design: fold and Basis lazy and reactive, one Rust process

Brief: `~/.lobby/ops/BRIEF-2026-11-08-fold-lazy-reactive-rust.md`. Facto prerequisite: `~/.lobby/ops/decisions/AUDIT-identity-keys-2026-11-08.md` F1-F3.
Status: design plus slice 1 (`src/reactive.rs`, feature `reactive`, off by default, nothing calls it yet).

## 1. Node identity

A node's identity is its **key**: the stable id, never a path, position, title, pid or clock.

- A node that has a source id: `<source>/<segment>/…` exactly as `src/id.rs` builds it (the haecceity path; segments are percent-encoded source ids).
- A node derived from a query: `q/<query id>/<params digest>/<row id>`, where the row id is the row's own id (facto vertex id, message id, landing id), never its index.
- A node with no id gets none here: it is an error to cache it (audit #13: `row-${i}`). Chunking (`part-N`) is by stable item ids, and boards are not chunked.

A node's **version** is the sha-256 (first 16 hex) of its output. An input's version is its facto vertex version. A cache entry is valid for (key, the exact (id, version) pairs it read). That is the whole cache key.

## 2. Reverse-dependency index

`Memo.readers: id -> {keys}` is rebuilt from each evaluation's recorded reads (`Reader::input`, `Reader::node`), replacing the node's old edges. An id here is either an input or another node, so one index serves the facto-to-node and the child-to-parent hops. A change marks readers dirty transitively; **dirty is not recompute**. A dirty node is revalidated when read: its own reads are brought up to date first and compared with what it recorded; only a real difference re-evaluates it. When a re-evaluation yields the same output its version does not move, so everything above it is revalidated, not recomputed (early cutoff; golden step 4). This is what stops "5,400 pages rewritten for `updated: now`": a clock is not an input, and a page whose output bytes are unchanged keeps its version, so the writer's "write only changed bytes" becomes "write nodes whose version moved".

## 3. What counts as "read soon"

Bounded, by rule, never "all nodes". Eager work is `get` called for:
1. the **Needs-you root** and its direct children after every applied feed batch (the one Bobby looks at);
2. every node with an **open long-poll or push subscription** (fold serve knows them);
3. nodes **pinned in Basis** (a small set stored by key).
Everything else is evaluated only when a request reads it. The budget per feed batch is the dirty set intersected with (1)-(3); the rest stays dirty and costs nothing until read.

## 4. facto feed contract

What fold-core needs (facto F1-F3, as the audit specifies):
- `GET /v1/graphs/{g}` carries `change_seq` (monotonic, bumped by statement-level triggers, no-op upserts do not bump) and an ETag.
- `GET /v1/graphs/{g}/changes?after=N&wait_ms=M` answers `{seq, changes: [{id, kind: vertex|edge, version}]}` for the changes after N, or an empty list with the same seq on timeout. `seq` of the batch is the graph's change_seq after it.
- An edge write bumps both endpoint vertices' versions (slice 2 of the audit), so a vertex id in `changes` covers its neighbourhood; until then an edge change is reported under the edge id and readers that traversed it must have recorded it (`Reader::input(edge id, version)`).
- Consumption (`Memo::feed`): `seq == last+1` applies; `seq <= last` is a redelivery and is ignored; a skip is a **gap**: every input version is forgotten and everything is dirty, so the next reads re-evaluate from facto. A restart starts at seq 0 with an empty memo (cold, lazy), then follows the feed. Pages are stamped "Data from seq N", not a wall clock (audit #2); the stamp is an input, so an unchanged page keeps its version.

Not assumed: delivery order within a batch, or that ids repeat; a duplicate change of the same (id, version) is a no-op.

## 5. Migration order and what each bun process becomes

One supervised Rust binary, one stint-host entry, one deadline policy (every `get` and every facto call runs under it; a hung tick cannot freeze a section, root cause of the 05:32-08:28 freeze). Each former process becomes a **set of node definitions** (`Eval` impls keyed by id prefix) in that binary, not a loop.

1. **Needs-you first.** `run-section.ts` (needs-you) becomes the `needs/…` nodes. Keyed by vertex version, so run-section and run-queries can no longer share one derivation record (audit root cause 1b). Flagged: the existing bun path stays default; the flag turns the Rust path on beside it and a golden compares their page.json.
2. `run-queries.ts` → `q/…` nodes (the `until` re-read is an input: a timer feeds `until` as a version, audit #11).
3. `run-trip.ts` → `trip/…` nodes (audit #4 disappears: no unsynced derive).
4. `needs-ingest.ts` and `prs-ingest.ts` → producers that write facto; they stay until their data is in the feed.
5. `fold.ts` (site writer) → nodes whose output is a page; the writer is "for each node whose version moved, write once" (audit #2, #3, #9, #16).
6. `fold serve` → already Rust; it subscribes to versions and pushes/long-polls on move (ETag exists).
Facto F1-F3 lands first; until it does, a poll adapter synthesises the feed from listings and reports the same `(id, version)` pairs, so slices 1-2 do not wait on facto.

## 6. Slice 1 (this PR)

`src/reactive.rs` (feature `reactive`): `Memo`, `Eval`, `Reader`, `Fed`, `Stats`. `tests/reactive.rs` plays a Needs-you scenario against `goldens/reactive/needs.log`: lazy cold reads (1-2), warm reads evaluate nothing (3), early cutoff (4), a real change re-evaluates only its path (5), redelivery (6), a gap (7), cycle and unknown node are errors (8). Run: `cargo test --features reactive --test reactive`.

Not in slice 1: facto client, the poll adapter, `fold serve` push, any node definitions, wasm exposure (the module is pure and builds for wasm32, the client can reuse it for open-page invalidation).

## 7. Open questions for Bobby

- Pinned-in-Basis set: stored where (facto vertex or a local file)? Proposal: a facto vertex per pin, so it is itself a node input.
- Is slice 2 the poll adapter plus the `needs/…` node definitions behind the flag? (proposed)
