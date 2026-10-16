# Acceptance examples and antipatterns

## Good single Task

Fix a reproducible macOS login redirect regression. Outputs: authentication-flow change, regression test, root-cause summary. Acceptance: reproduction passes, focused tests pass, target builds, adjacent behavior remains covered. Rationale: one small module and one capability set make implementation and verification tightly coupled.

## Good graph

Goal: ship a shared Task-contract change across Rust Host and macOS. `contract` defines schema; `rust-host` and `mac-client` depend on it and own disjoint paths; `integration-qa` depends on both and runs cross-boundary checks. The contract gates parallel implementations and integration is independently verifiable.

## Antipatterns

- One Task called "complete everything" with unrelated outputs.
- Separate "analyze", "think", and "plan" Tasks without useful artifacts.
- A linear chain representing preferred order rather than required input.
- Parallel writers with overlapping ownership.
- Acceptance such as "looks good" or "works" without evidence.
- `context_refs` used when a completed upstream result is required.
- Every Task receives every plugin regardless of need.
- Verification merely repeats the implementation summary.
- Historical completion is presented as proof of a new change.
- A read-only request is rewritten to prohibit screenshots, snapshots, reads, or listings that the user never prohibited, making the selected inspection capability unusable.
