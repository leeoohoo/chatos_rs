# API Contract Source-of-Truth

This directory bootstraps an OpenAPI-based contract workflow.

## Scope

- `chat_app_server_rs.openapi.yaml`: main backend API contract draft
- `memory_server.openapi.yaml`: memory backend API contract draft
- `fragments/`: domain-split OpenAPI source files for low-conflict collaboration
- `ownership/`: manifest-driven owner mapping metadata

## Ownership

- Main backend contract: `chat_app_server_rs` maintainers
- Memory backend contract: `memory_server/backend` maintainers
- CI wiring and baseline scripts: repository/platform maintainers
- Path-level responsibility map: `OWNERSHIP_MAP.md`

## Current Stage

- Stage: enforcement (M3 threshold reached)
- Route/path baselines remain strict drift guards.
- OpenAPI gate is policy-driven via:
  - policy file: `openapi-gate-policy.env`
  - assembly check: `scripts/check_openapi_contract_assembly.sh`
  - owner policy check: `scripts/check_openapi_fragment_owner_policy.sh`
  - ownership consistency check: `scripts/check_openapi_ownership_map_consistency.sh`
  - owner-report schema check: `scripts/check_openapi_owner_report_schema.sh`
  - gate script: `scripts/check_openapi_contract_gate.sh`
  - method gate script: `scripts/check_openapi_method_contract_gate.sh`
  - quality gate script: `scripts/check_openapi_contract_quality_gate.sh`
  - semantic gate script: `scripts/check_openapi_contract_semantic_gate.sh`
  - emergency waiver folder: `waivers/`
- Waiver lifecycle is bounded by policy (`OPENAPI_GATE_WAIVER_MAX_HOURS`, default `24h`).
- Current required coverage floor:
  - main backend >= `60%`
  - memory backend >= `60%`
- Current required method-coverage floor:
  - main backend >= `95%`
  - memory backend >= `95%`
- Current required quality floor:
  - summary ratio >= `100%`
  - path-parameter completeness >= `100%`
  - response-description ratio >= `100%`
  - success-response ratio >= `100%` (2xx or websocket 101)
- Current required semantic floor:
  - operationId ratio >= `100%`
  - operationId uniqueness ratio >= `100%`
  - success-json-schema ratio >= `100%`
  - requestBody-json-schema ratio >= `100%`
- Contract semantic baseline:
  - success responses use shared component ref `#/components/schemas/StandardResponse` by default.
- Optional CODEOWNERS mapping:
  - `.github/CODEOWNERS.openapi` (can be merged into `.github/CODEOWNERS` when enabling auto-review request)
- Ownership single source manifest:
  - `.github/api-contract/ownership/manifest.yaml`

## Migration Path

1. Edit fragment files under `fragments/` by bounded domain.
2. Rebuild assembled contracts with `bash scripts/update_openapi_contract_assembly.sh`.
3. Keep route/path baselines and all required gates green.
4. Continue hardening method/schema quality and tighten waiver policy over time.

## Automation Helpers

- Local pre-commit quick-run:
  - `bash scripts/precommit_openapi_contracts.sh`
  - fast mode: `OPENAPI_PRECOMMIT_FAST_MODE=true bash scripts/precommit_openapi_contracts.sh`
- Diff-scoped fast checks (pre-push friendly):
  - `bash scripts/check_openapi_contract_fast_diff.sh`
- Owner-aware fragment report:
  - `bash scripts/report_openapi_fragment_owners.sh`
  - machine-readable: `bash scripts/report_openapi_fragment_owners.sh --json`
- Ownership map/CODEOWNERS consistency gate:
  - `bash scripts/check_openapi_ownership_map_consistency.sh`
  - source-of-truth manifest: `.github/api-contract/ownership/manifest.yaml`
  - discrepancy artifact: `OPENAPI_OWNERSHIP_DISCREPANCY_OUTPUT=/tmp/openapi-ownership-discrepancy.json`
- Owner-report schema validation:
  - `bash scripts/check_openapi_owner_report_schema.sh`
- Contract change summary artifact (markdown):
  - `bash scripts/generate_openapi_contract_change_summary.sh`
  - also emits JSON summary and optional PR-comment draft artifact
- Ownership drift trend snapshot (base/head):
  - `bash scripts/generate_openapi_ownership_drift_trend.sh`
- Governance bundle (single-entry reviewer/bot package):
  - `bash scripts/generate_openapi_governance_bundle.sh`
  - emits `GOVERNANCE_SUMMARY.md` + `artifact-index.json`
  - can reuse previously generated CI/local artifacts to avoid recomputation:
    - `OPENAPI_GOVERNANCE_BUNDLE_REUSE_EXISTING=true`
    - `OPENAPI_CHANGE_SUMMARY_INPUT=/path/to/openapi-contract-change-summary.md`
    - `OPENAPI_OWNER_REPORT_INPUT=/path/to/openapi-contract-change-summary.json`
    - `OPENAPI_PR_COMMENT_DRAFT_INPUT=/path/to/openapi-contract-pr-comment.md`
    - `OPENAPI_OWNERSHIP_DISCREPANCY_INPUT=/path/to/openapi-ownership-discrepancy.json`
    - `OPENAPI_OWNERSHIP_DRIFT_TREND_INPUT=/path/to/openapi-ownership-drift-trend.md`
    - `OPENAPI_OWNERSHIP_DRIFT_TREND_JSON_INPUT=/path/to/openapi-ownership-drift-trend.json`
- Governance bundle integrity validation:
  - `bash scripts/check_openapi_governance_bundle_integrity.sh`
- Owner JSON schema:
  - `.github/api-contract/ownership/owner-report.schema.json`
  - `.github/api-contract/ownership/governance-bundle-index.schema.json`
