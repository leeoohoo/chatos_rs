# Project High-Value Optimization Plan (2026-04-10)

## 0) Goal

Prioritize optimizations with the highest engineering ROI:

- reduce regression risk in multi-service delivery,
- tighten frontend-backend contract consistency,
- improve production-readiness and observability,
- avoid low-impact cosmetic refactors.

This plan is execution-oriented and updated phase-by-phase.

## 1) Baseline Snapshot

- Monorepo with 4 major services:
  - `chat_app/` (React + TypeScript)
  - `chat_app_server_rs/` (Rust, main orchestration backend)
  - `memory_server/` (Rust backend + React frontend)
  - `openai-codex-gateway/` (Python gateway)
- Current CI coverage exists but is not yet end-to-end complete for all quality gates.
- Gateway and memory-frontend quality gates are partially missing from CI policy.

## 2) Phase Roadmap

### Phase 1 (Completed): CI Completeness First

Objective:

- add gateway test gate into CI,
- add memory frontend type-check gate and enforce it in CI.

Planned changes:

- update `.github/workflows/ci.yml`
- update `memory_server/frontend/package.json` scripts

Verification:

- `npm run type-check` under `memory_server/frontend`
- gateway regression command in CI-compatible form

### Phase 2 (In Progress): Frontend Bundle Performance

Objective:

- reduce initial bundle size for `chat_app` and `memory_server/frontend`.

Planned directions:

- lazy-load heavy rendering dependencies (e.g. mermaid/katex/cytoscape paths),
- introduce chunk strategy (`manualChunks`) and bundle budget checks.

### Phase 3 (In Progress): API Contract Standardization

Objective:

- establish stable API contract layer between frontend and Rust services.

Planned directions:

- introduce OpenAPI/Schema-based contract generation,
- generate typed client artifacts,
- add contract drift checks in CI.

### Phase 4 (In Progress): Runtime/DevOps Robustness

Objective:

- improve local/ops startup reliability and reduce false-positive “alive” checks.

Planned directions:

- replace fixed `sleep` startup waits with active health checks,
- refine process stop strategy to reduce port-based collateral kill risk,
- align root quickstart and compose topology documentation.

### Phase 5 (In Progress): Security & Dependency Governance

Objective:

- reduce supply-chain and dependency drift risk.

Planned directions:

- add dependency update policy (`dependabot` or equivalent),
- add periodic audit tasks for npm/cargo/pip ecosystems.

## 3) Progress Log

- 2026-04-10: Plan initialized at repository root.
- 2026-04-10: Phase 1 started.
- 2026-04-10: Phase 1 completed.
  - Added gateway CI job in `.github/workflows/ci.yml`:
    - Python setup
    - `pip install -r openai-codex-gateway/requirements.txt`
    - `make test`
    - `python server.py --help`
  - Added `type-check` script to `memory_server/frontend/package.json`.
  - Added memory frontend CI type-check step.
  - Local validation passed:
    - `npm run type-check` (`memory_server/frontend`)
    - `make test` (`openai-codex-gateway`, 102 tests)
    - `python server.py --help` (`openai-codex-gateway`)
- 2026-04-10: Phase 2 Step 1 completed (chat_app bundle split baseline).
  - Added Rollup `manualChunks` strategy in `chat_app/vite.config.ts`.
  - Introduced lazy markdown rendering wrapper:
    - `chat_app/src/components/LazyMarkdownRenderer.tsx`
  - Replaced eager `MarkdownRenderer` usage in core render paths:
    - `MessageItem.tsx`
    - `messageItem/MessageContentRenderer.tsx`
    - `ToolCallRenderer.tsx`
    - `notepad/NotepadEditor.tsx`
    - `chatInterface/SummaryPane.tsx`
  - Build impact (chat_app):
    - main index chunk: `~638.7 kB` -> `~630.9 kB`
    - markdown renderer moved to async chunk: `MarkdownRenderer-*.js` (`~8.38 kB`)
- 2026-04-10: Phase 2 Step 2 completed (memory frontend chunk strategy baseline).
  - Added chunk split strategy in `memory_server/frontend/vite.config.ts`:
    - `vendor-antd`
    - `vendor-axios`
  - Resolved intermediate circular chunk warning by simplifying manual chunk rules.
  - Build impact (memory_server/frontend):
    - main index chunk: `~1250.1 kB` -> `~119.3 kB`
    - large vendor moved to `vendor-antd` chunk (`~1093.4 kB`)
- 2026-04-10: Phase 4 Step 1 completed (startup health checks).
  - Upgraded `restart_services.sh` from fixed sleep-only to active HTTP readiness probing.
  - Added `wait_http_ready` checks for:
    - main backend `/health`
    - main frontend `/`
    - memory backend `/health`
    - memory frontend `/`
  - Added configurable timeout: `STARTUP_HEALTHCHECK_TIMEOUT_SEC` (default: `45`).
  - Validation:
    - `bash -n restart_services.sh`
    - `bash restart_services.sh restart` succeeded with health-check pass logs.
- 2026-04-10: Phase 5 Step 1 completed (dependency governance bootstrap).
  - Added `.github/dependabot.yml` for weekly updates:
    - npm: `chat_app`, `memory_server/frontend`
    - cargo: `chat_app_server_rs`, `memory_server/backend`
    - pip: `openai-codex-gateway`
- 2026-04-10: Phase 3 Step 1 completed (API surface baseline + CI drift guard).
  - Added scripts:
    - `scripts/generate_api_surface_snapshot.sh`
    - `scripts/update_api_surface_baseline.sh`
    - `scripts/check_api_surface.sh`
  - Added baseline artifact:
    - `.github/api-surface-baseline.txt`
  - Added CI gate:
    - `.github/workflows/ci.yml` -> `API Surface Contract` job (`bash scripts/check_api_surface.sh`)
  - Baseline snapshot captured:
    - `main_backend_route_count=111`
    - `memory_backend_route_count=53`
    - `total_route_count=164`
- 2026-04-10: Phase 3 Step 2 completed (path-level contract baseline).
  - Added scripts:
    - `scripts/generate_api_path_snapshot.sh`
    - `scripts/update_api_path_baseline.sh`
    - `scripts/check_api_path_baseline.sh`
  - Added normalized path baseline artifact:
    - `.github/api-path-baseline.txt`
  - Added CI gate:
    - `.github/workflows/ci.yml` -> `Verify API path baseline`
  - Baseline snapshot captured:
    - `main_backend_endpoint_count=152`
    - `memory_backend_endpoint_count=70`
    - `total_endpoint_count=222`
- 2026-04-10: Phase 3 Step 3 completed (OpenAPI bootstrap + advisory CI).
  - Added OpenAPI contract bootstrap files:
    - `.github/api-contract/README.md`
    - `.github/api-contract/chat_app_server_rs.openapi.yaml`
    - `.github/api-contract/memory_server.openapi.yaml`
  - Added advisory script:
    - `scripts/check_openapi_contract_advisory.sh`
  - CI integration (non-blocking policy by script semantics):
    - `.github/workflows/ci.yml` -> `OpenAPI contract advisory`
  - Advisory snapshot:
    - main backend baseline endpoints: `152`, openapi paths: `1`
    - memory backend baseline endpoints: `70`, openapi paths: `1`
- 2026-04-10: Phase 3 Step 4 started (contract enrichment execution).
  - Enriched OpenAPI drafts with high-frequency session/message endpoints:
    - `.github/api-contract/chat_app_server_rs.openapi.yaml`
    - `.github/api-contract/memory_server.openapi.yaml`
  - Added coverage target document:
    - `.github/api-contract/COVERAGE_TARGETS.md`
  - Upgraded advisory script output with coverage ratio:
    - `scripts/check_openapi_contract_advisory.sh`
  - Current advisory snapshot:
    - main backend: `5 / 152` (`3.29%`)
    - memory backend: `5 / 70` (`7.14%`)
- 2026-04-10: Phase 3 Step 4 progressed (projects/contacts contract expansion).
  - Expanded `chat_app_server_rs` OpenAPI coverage for:
    - `/api/projects*`
    - `/api/contacts*`
  - Expanded `memory_server` OpenAPI coverage for:
    - `/api/memory/v1/contacts*`
    - `/api/memory/v1/projects*`
    - `/api/memory/v1/project-agent-links/sync`
  - Updated advisory snapshot:
    - main backend: `21 / 152` (`13.82%`)
    - memory backend: `16 / 70` (`22.86%`)
- 2026-04-10: Phase 3 Step 4 milestone reached (M2 >=30% coverage).
  - Expanded main backend OpenAPI coverage with:
    - `/api/auth/*`
    - `/api/agent_v2/*`
    - `/api/memory-agents*`
    - `/api/system-context*`
    - `/api/ui-prompts/*`
  - Updated advisory snapshot:
    - main backend: `53 / 152` (`34.87%`)
    - memory backend: `27 / 70` (`38.57%`)
  - M2 coverage target (`>=30%`) is now achieved for both services.
- 2026-04-10: Phase 3 Step 5 completed (policy-driven required gate + governance docs).
  - Added OpenAPI gate policy:
    - `.github/api-contract/openapi-gate-policy.env`
  - Added required gate script with emergency waiver support:
    - `scripts/check_openapi_contract_gate.sh`
  - Added waiver governance assets:
    - `.github/api-contract/waivers/README.md`
    - `.github/api-contract/waivers/openapi_gate_waiver.example.env`
  - Added API path ownership map:
    - `.github/api-contract/OWNERSHIP_MAP.md`
  - Added PR contract checklist template:
    - `.github/pull_request_template.md`
  - CI integration:
    - `.github/workflows/ci.yml` -> `OpenAPI contract gate`
  - Local gate snapshot at policy floor (required, 30% at this step):
    - main backend: `53 / 152` (`34.87%`) -> pass
    - memory backend: `27 / 70` (`38.57%`) -> pass
- 2026-04-10: Phase 3 Step 6 completed (coverage expansion + floor raise to 40%).
  - Expanded main backend OpenAPI coverage for high-risk write/control paths:
    - `/api/ai-model-configs*`
    - `/api/mcp-configs*`
    - `/api/session-summary-job-config`
    - `/api/sessions/{session_id}/messages`
    - `/api/sessions/{session_id}/summaries`
    - `/api/system-contexts/{context_id}/activate`
    - `/api/ui-prompts/{prompt_id}/respond`
    - `/api/task-manager/tasks/{task_id}/complete`
  - Expanded memory backend OpenAPI coverage for:
    - `/api/memory/v1/auth/login`
    - `/api/memory/v1/auth/me`
    - `/api/memory/v1/skills/plugins/install`
  - Corrected inaccurate memory contract draft operation:
    - removed `DELETE` on `/api/memory/v1/jobs/stats`
  - Raised required floor in gate policy:
    - `OPENAPI_MAIN_MIN_COVERAGE_RATIO=40`
    - `OPENAPI_MEMORY_MIN_COVERAGE_RATIO=40`
  - Updated snapshot after expansion:
    - main backend: `63 / 152` (`41.45%`)
    - memory backend: `30 / 70` (`42.86%`)
  - Required gate (40%) passes locally.
- 2026-04-10: Phase 3 Step 7 completed (coverage expansion + floor raise to 50%).
  - Expanded main backend OpenAPI coverage for:
    - `/api/notepad/*`
    - `/api/fs/list`
    - `/api/fs/entries`
    - `/api/fs/read`
    - `/api/fs/search`
    - `/api/remote-connections*`
  - Expanded memory backend OpenAPI coverage for:
    - `/api/memory/v1/agents*`
    - `/api/memory/v1/auth/users*`
    - `/api/memory/v1/skills`
  - Raised required floor in gate policy:
    - `OPENAPI_MAIN_MIN_COVERAGE_RATIO=50`
    - `OPENAPI_MEMORY_MIN_COVERAGE_RATIO=50`
  - Updated snapshot after expansion:
    - main backend: `78 / 152` (`51.32%`)
    - memory backend: `37 / 70` (`52.86%`)
  - Required gate (50%) passes locally.
- 2026-04-10: Phase 3 Step 8 progressed (waiver lifecycle hardening).
  - Added policy knob:
    - `.github/api-contract/openapi-gate-policy.env`
    - `OPENAPI_GATE_WAIVER_MAX_HOURS=24`
  - Enforced waiver max-lifetime in gate script:
    - `scripts/check_openapi_contract_gate.sh`
  - Updated governance docs:
    - `.github/api-contract/README.md`
    - `.github/api-contract/COVERAGE_TARGETS.md`
    - `.github/api-contract/waivers/README.md`
  - Validation:
    - required gate remains green at 50% floor.
- 2026-04-10: Phase 3 Step 9 completed (M3 threshold achieved and enforced).
  - Expanded main backend OpenAPI coverage to include remaining high-risk domains:
    - `/api/fs/*` write/download operations
    - `/api/remote-connections/*` SFTP + websocket operations
    - `/api/sessions/{session_id}/mcp-servers*`
    - `/api/sessions/{session_id}/turns/*/process`
    - `/api/sessions/{session_id}/turns/*/runtime-context`
    - `/api/system-contexts/ai/*`
    - `/api/user-settings`
    - `/api/agent-builder/ai-create`
    - root app entry paths (`/`, `/{application_id}`)
  - Expanded memory backend OpenAPI coverage to include:
    - `/api/memory/v1/agents/ai-create`
    - `/api/memory/v1/context/compose`
    - `/api/memory/v1/sessions/{session_id}/messages/batch`
    - `/api/memory/v1/sessions/{session_id}/messages/{message_id}/sync`
    - `/api/memory/v1/sessions/{session_id}/summaries*`
    - `/api/memory/v1/sessions/{session_id}/turn-runtime-snapshots*`
    - `/api/memory/v1/skills/{skill_id}`
    - `/api/memory/v1/skills/plugins*`
    - `/api/memory/v1/skills/import-git`
  - Updated coverage snapshot:
    - main backend: `111 / 152` (`73.03%`)
    - memory backend: `53 / 70` (`75.71%`)
  - Raised required floor in gate policy to M3 threshold:
    - `OPENAPI_MAIN_MIN_COVERAGE_RATIO=60`
    - `OPENAPI_MEMORY_MIN_COVERAGE_RATIO=60`
  - Required gate (60%) passes locally.
- 2026-04-10: Phase 3 Step 10 progressed (method-level contract gate).
  - Added method-level gate script:
    - `scripts/check_openapi_method_contract_gate.sh`
  - Added policy knobs:
    - `OPENAPI_METHOD_GATE_MODE=required`
    - `OPENAPI_METHOD_MAIN_MIN_COVERAGE_RATIO=95`
    - `OPENAPI_METHOD_MEMORY_MIN_COVERAGE_RATIO=95`
  - CI integration:
    - `.github/workflows/ci.yml` -> `OpenAPI method contract gate`
  - Snapshot at rollout:
    - main backend method coverage: `147 / 152` (`96.71%`)
    - memory backend method coverage: `69 / 70` (`98.57%`)
  - Method gate passes locally.
- 2026-04-10: Phase 3 Step 11 progressed (contract structural quality gate).
  - Added quality gate script:
    - `scripts/check_openapi_contract_quality_gate.sh`
  - Added policy knobs:
    - `OPENAPI_QUALITY_GATE_MODE=required`
    - `OPENAPI_QUALITY_MIN_SUMMARY_RATIO=100`
    - `OPENAPI_QUALITY_MIN_PATH_PARAM_RATIO=100`
    - `OPENAPI_QUALITY_MIN_RESPONSE_DESC_RATIO=100`
    - `OPENAPI_QUALITY_MIN_SUCCESS_RESPONSE_RATIO=100`
  - CI integration:
    - `.github/workflows/ci.yml` -> `OpenAPI quality gate`
  - Snapshot at rollout:
    - main: summary/path-param/response-desc/success-response = `100%`
    - memory: summary/path-param/response-desc/success-response = `100%`
  - Quality gate passes locally.
- 2026-04-10: Phase 3 Step 12 progressed (semantic schema hardening gate).
  - Normalized OpenAPI semantic metadata for both contracts:
    - added `operationId` for all operations
    - added success-response JSON schema attachment via `#/components/schemas/StandardResponse`
  - Added semantic gate script:
    - `scripts/check_openapi_contract_semantic_gate.sh`
  - Added policy knobs:
    - `OPENAPI_SEMANTIC_GATE_MODE=required`
    - `OPENAPI_SEMANTIC_MIN_OPERATION_ID_RATIO=100`
    - `OPENAPI_SEMANTIC_MIN_OPERATION_ID_UNIQUENESS_RATIO=100`
    - `OPENAPI_SEMANTIC_MIN_SUCCESS_JSON_SCHEMA_RATIO=100`
    - `OPENAPI_SEMANTIC_MIN_REQUEST_BODY_JSON_SCHEMA_RATIO=100`
  - CI integration:
    - `.github/workflows/ci.yml` -> `OpenAPI semantic gate`
  - Snapshot at rollout:
    - main: operationId/uniqueness/success-json-schema/requestBody-json-schema = `100%`
    - memory: operationId/uniqueness/success-json-schema/requestBody-json-schema = `100%`
  - Semantic gate passes locally.
- 2026-04-10: Phase 3 Step 13 completed (contract modularization hardening).
  - Introduced OpenAPI fragment structure:
    - `.github/api-contract/fragments/chat_app_server_rs/*`
    - `.github/api-contract/fragments/memory_server/*`
  - Added assembly tooling:
    - `scripts/assemble_openapi_contracts.py`
    - `scripts/check_openapi_contract_assembly.sh`
    - `scripts/update_openapi_contract_assembly.sh`
  - CI integration:
    - `.github/workflows/ci.yml` -> `Verify OpenAPI assembly`
  - Added fragment workflow documentation:
    - `.github/api-contract/fragments/README.md`
  - Assembled contract outputs are rebuilt from fragments and tracked in:
    - `.github/api-contract/chat_app_server_rs.openapi.yaml`
    - `.github/api-contract/memory_server.openapi.yaml`
  - Assembly check passes locally.
- 2026-04-10: Phase 3 Step 14 completed (governance automation hardening).
  - Added local quick-run helper:
    - `scripts/precommit_openapi_contracts.sh`
  - Added owner-aware fragment report:
    - `scripts/report_openapi_fragment_owners.sh`
  - Added markdown change summary generator:
    - `scripts/generate_openapi_contract_change_summary.sh`
  - CI integration:
    - fetch-depth upgraded to `0` in API contract job
    - owner report step added in logs
    - change summary artifact generation + upload added
  - Validation:
    - precommit quick-run passes locally
    - summary artifact generation works locally (`/tmp/openapi-contract-change-summary.md`)
- 2026-04-10: Phase 3 Step 15 completed (differential gate optimization).
  - Added diff-scoped fast check script:
    - `scripts/check_openapi_contract_fast_diff.sh`
  - Fast-check strategy:
    - deterministic diff range resolution (`explicit ref -> origin/main merge-base -> HEAD~1 -> HEAD`)
    - changed-path scoped execution for OpenAPI/assembly/baseline checks
    - conservative full fallback for unknown/high-risk changes
  - Extended precommit helper:
    - `OPENAPI_PRECOMMIT_FAST_MODE=true bash scripts/precommit_openapi_contracts.sh`
  - Validation:
    - fast script passes with explicit empty diff (`HEAD..HEAD`)
    - fast mode passes and correctly falls back to full checks on high-risk diff
- 2026-04-10: Phase 3 Step 16 completed (ownership-rule enforcement hardening).
  - Added optional CODEOWNERS mapping aligned with fragment domains:
    - `.github/CODEOWNERS.openapi`
  - Added strict owner policy gate:
    - `scripts/check_openapi_fragment_owner_policy.sh`
    - (backed by `scripts/report_openapi_fragment_owners.sh --strict`)
  - Added reviewer owner-confirmation checklist section in change summary artifact:
    - `scripts/generate_openapi_contract_change_summary.sh`
  - CI integration:
    - `.github/workflows/ci.yml` -> `OpenAPI fragment owner policy`
  - Validation:
    - strict owner policy script passes locally
    - summary artifact includes reviewer checklist section
- 2026-04-10: Phase 3 Step 17 completed (reviewer automation refinement).
  - Added machine-readable owner report output:
    - `scripts/report_openapi_fragment_owners.sh --json`
  - Extended change summary generation to emit structured artifacts:
    - markdown summary + JSON summary + optional PR comment draft
    - `scripts/generate_openapi_contract_change_summary.sh`
  - Added ownership discrepancy detector (OWNERSHIP_MAP vs CODEOWNERS):
    - `scripts/check_openapi_ownership_map_consistency.sh`
  - CI integration:
    - `.github/workflows/ci.yml` -> `OpenAPI ownership map consistency`
    - upload artifact: `openapi-contract-change-summary-json`
    - upload artifact: `openapi-contract-pr-comment-draft`
  - Local integration:
    - `scripts/check_openapi_contract_fast_diff.sh` now runs owner policy + ownership consistency when ownership scope changes
    - `scripts/precommit_openapi_contracts.sh` now includes ownership gates
  - Validation:
    - owner report `--json` works locally
    - ownership consistency gate passes locally
    - summary generation emits markdown/json/comment artifacts locally
- 2026-04-10: Phase 3 Step 18 completed (ownership mapping single-source hardening).
  - Added ownership source-of-truth manifest:
    - `.github/api-contract/ownership/manifest.yaml`
  - Refactored owner report to load fragment owner hints/footprints from manifest:
    - `scripts/report_openapi_fragment_owners.sh`
  - Refactored ownership consistency gate to load expected path-prefix scope and CODEOWNERS mapping from manifest:
    - `scripts/check_openapi_ownership_map_consistency.sh`
  - Fast diff integration:
    - ownership manifest changes now trigger ownership-governance checks
    - `scripts/check_openapi_contract_fast_diff.sh`
  - Governance mapping update:
    - `.github/CODEOWNERS.openapi` adds manifest owner entry
  - Validation:
    - report script markdown/tsv/json outputs pass locally
    - ownership consistency gate passes with manifest-driven checks
    - openapi precommit full + fast modes stay green
- 2026-04-10: Phase 3 Step 19 completed (owner-review automation consumability hardening).
  - Added owner-report schema versioning contract:
    - JSON payload now includes `schema_version=openapi.owner_report.v1`
    - schema file: `.github/api-contract/ownership/owner-report.schema.json`
  - Added lightweight owner JSON schema validator:
    - `scripts/check_openapi_owner_report_schema.sh`
  - CI integration:
    - `.github/workflows/ci.yml` -> `Validate OpenAPI owner report schema`
  - Local integration:
    - `scripts/precommit_openapi_contracts.sh` includes owner-report schema gate
    - `scripts/check_openapi_contract_fast_diff.sh` runs schema validation for ownership-governance diffs
  - PR comment idempotency hooks:
    - `scripts/generate_openapi_contract_change_summary.sh` now emits
      - `<!-- openapi-owner-checklist:start version=v1 -->`
      - `<!-- openapi-owner-checklist:end -->`
  - Validation:
    - owner-report schema validation passes for both direct report output and change-summary JSON artifact
    - full OpenAPI governance gates and precommit modes remain green
- 2026-04-10: Phase 3 Step 20 completed (ownership drift observability enhancement).
  - Added machine-readable discrepancy artifact from ownership consistency check:
    - `scripts/check_openapi_ownership_map_consistency.sh`
    - output via `OPENAPI_OWNERSHIP_DISCREPANCY_OUTPUT`
  - Added discrepancy severity classification for bot triage:
    - `missing_fragment`
    - `missing_codeowner`
    - `scope_mismatch`
  - Added historical trend snapshot generator (base/head drift comparison):
    - `scripts/generate_openapi_ownership_drift_trend.sh`
    - markdown + JSON outputs
  - CI integration:
    - upload artifact: `openapi-ownership-discrepancy`
    - upload artifact: `openapi-ownership-drift-trend`
  - Validation:
    - consistency gate still fails/pass as expected while emitting structured artifact
    - trend snapshot generation works with resolved base/head refs
    - full OpenAPI precommit/fast/full gate chain remains green
- 2026-04-10: Phase 3 Step 21 completed (contract governance artifact consolidation).
  - Added one-shot governance bundle generator:
    - `scripts/generate_openapi_governance_bundle.sh`
  - Bundle contents (single-entry reviewer/bot package):
    - `contract-change-summary.md`
    - `owner-report.json`
    - `ownership-discrepancy.json`
    - `ownership-drift-trend.md`
    - `ownership-drift-trend.json`
    - `pr-comment-draft.md`
    - `artifact-index.json` (stable keys + schema versions)
    - `GOVERNANCE_SUMMARY.md` (single-entry summary)
  - CI integration:
    - upload artifact: `openapi-governance-bundle`
  - Validation:
    - bundle generation works locally with resolved base/head refs
    - bundle index and summary are generated and parseable
    - full OpenAPI precommit/fast/full gate chain remains green
- 2026-04-10: Phase 3 Step 22 completed (governance bundle schema hardening).
  - Added JSON schema for governance bundle index:
    - `.github/api-contract/ownership/governance-bundle-index.schema.json`
  - Added governance bundle integrity validator:
    - `scripts/check_openapi_governance_bundle_integrity.sh`
    - validates required files + index schema_version + inner artifact schema versions
  - CI integration:
    - `.github/workflows/ci.yml` -> `Validate OpenAPI governance bundle integrity`
  - Validation:
    - governance bundle generation + integrity check pass locally
    - full OpenAPI precommit/fast/full gate chain remains green
- 2026-04-10: Phase 3 Step 23 completed (governance signal de-duplication and latency reduction).
  - Added shared-artifact handoff/reuse support in governance bundle generator:
    - `scripts/generate_openapi_governance_bundle.sh`
    - new reuse inputs:
      - `OPENAPI_GOVERNANCE_BUNDLE_REUSE_EXISTING`
      - `OPENAPI_CHANGE_SUMMARY_INPUT`
      - `OPENAPI_OWNER_REPORT_INPUT`
      - `OPENAPI_PR_COMMENT_DRAFT_INPUT`
      - `OPENAPI_OWNERSHIP_DISCREPANCY_INPUT`
      - `OPENAPI_OWNERSHIP_DRIFT_TREND_INPUT`
      - `OPENAPI_OWNERSHIP_DRIFT_TREND_JSON_INPUT`
  - Fallback hardening:
    - when upstream always-run steps fail and source artifacts are missing, bundle generation now writes schema-compatible placeholder artifacts instead of dropping observability outputs.
  - CI integration:
    - `.github/workflows/ci.yml` now passes `${{ runner.temp }}` outputs into governance bundle step for reuse-first assembly, reducing duplicated summary/discrepancy/trend computation.
  - Validation:
    - bundle generation works in reuse mode with existing artifacts
    - governance bundle integrity check remains green
    - existing OpenAPI artifact upload topology unchanged (`if: always()` preserved)

## 4) Current Next Step

- Execute Phase 3 (Step 24): local/fast governance parity enhancement
  - add diff-scoped/local governance-bundle smoke checks (generation + integrity) into fast/precommit flows where applicable,
  - ensure local developer workflow catches governance bundle regressions before CI,
  - keep fast path selective to avoid unnecessary overhead on docs-only diffs.
