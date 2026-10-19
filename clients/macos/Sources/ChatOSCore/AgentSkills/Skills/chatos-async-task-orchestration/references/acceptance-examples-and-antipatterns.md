# Acceptance examples and antipatterns

## Good single Task: known cohesive fix

The failing null guard in `src/config.ts::loadConfig` and its expected behavior are already identified. Outputs: the focused change, regression test, and short cause note. Acceptance: the focused test passes and the adjacent configuration behavior remains covered. Rationale: one small module, one write owner, one capability set, and a check that is part of implementing the known change. Separate search, edit, and test Tasks would create handoffs without independent value.

## Good graph: unknown UI failures

Goal: several user-management page actions are reported unusable, but the affected layers and causes are unknown. `diagnose` uses the real browser and terminal without project write authority to produce a reproducible defect list, root-cause evidence, and likely paths. `fix` depends on that evidence, receives code-write and terminal capabilities, and owns only the confirmed mutations plus focused tests. `verify` depends on the fix, receives browser and terminal observation capabilities without code write, and independently replays the visible actions and regression checks. Rationale: each handoff changes the next Task's scope or authority, and independent acceptance would be weakened if the verifier repaired its own findings.

## Good graph: shared contract and parallel work

Goal: ship a shared Task-contract change across Rust Host and macOS. `contract` defines schema; `rust-host` and `mac-client` depend on it and own disjoint paths; `integration-qa` depends on both and runs cross-boundary checks. The contract gates parallel implementations and integration is independently verifiable.

## Bad graph: procedural decomposition

`search-files -> read-files -> edit-file -> run-test` is not meaningful decomposition when one worker, one context, and one narrow capability set can own the change. Those are internal actions, not independently accepted outcomes. Keep them in one Task.

## Bad single Task: catch-all authority

“Investigate every broken UI action, edit any relevant frontend/backend/database file, operate the browser, run every command, and certify the result independently” hides several unknown ownership boundaries inside one worker and grants every capability up front. Split when diagnosis must establish the write scope and final acceptance must remain independent.

## Antipatterns

- One Task called "complete everything" with unrelated outputs.
- Separate "analyze", "think", and "plan" Tasks without useful artifacts.
- A linear chain representing preferred order rather than required input.
- Parallel writers with overlapping ownership.
- Acceptance such as "looks good" or "works" without evidence.
- `context_refs` used when a completed upstream result is required.
- Every Task receives every plugin regardless of need.
- A graph is forced into a fixed number of Tasks even though some phases have no independent value.
- Verification merely repeats the implementation summary.
- Historical completion is presented as proof of a new change.
- A read-only request is rewritten to prohibit screenshots, snapshots, reads, or listings that the user never prohibited, making the selected inspection capability unusable.
