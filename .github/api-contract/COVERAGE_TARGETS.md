# OpenAPI Coverage Targets (Bootstrap Milestones)

## Current Strategy

- Route/path baselines are strict CI guards.
- OpenAPI fragment assembly parity is enforced:
  - `scripts/check_openapi_contract_assembly.sh`
- OpenAPI has a required floor gate, configured by:
  - `.github/api-contract/openapi-gate-policy.env`
  - `scripts/check_openapi_contract_gate.sh`
- OpenAPI method-level coverage has a required gate:
  - `scripts/check_openapi_method_contract_gate.sh`
- OpenAPI structural quality has a required gate:
  - `scripts/check_openapi_contract_quality_gate.sh`
- OpenAPI semantic quality has a required gate:
  - `scripts/check_openapi_contract_semantic_gate.sh`
- Local diff-scoped fast gate (pre-push friendly):
  - `scripts/check_openapi_contract_fast_diff.sh`
- CI owner-aware fragment report:
  - `scripts/report_openapi_fragment_owners.sh`
- CI owner footprint policy gate:
  - `scripts/check_openapi_fragment_owner_policy.sh`
- CI ownership-map/CODEOWNERS consistency gate:
  - `scripts/check_openapi_ownership_map_consistency.sh`
  - (driven by `.github/api-contract/ownership/manifest.yaml`)
- CI ownership discrepancy artifact:
  - from `scripts/check_openapi_ownership_map_consistency.sh`
  - severity classes: `missing_fragment`, `missing_codeowner`, `scope_mismatch`
- CI markdown change summary artifact:
  - `scripts/generate_openapi_contract_change_summary.sh`
  - includes machine-readable owner JSON + PR comment draft artifact
- CI owner-report JSON schema gate:
  - `scripts/check_openapi_owner_report_schema.sh`
- CI ownership drift trend snapshot artifact:
  - `scripts/generate_openapi_ownership_drift_trend.sh`
- CI consolidated governance bundle artifact:
  - `scripts/generate_openapi_governance_bundle.sh`
- CI governance bundle integrity gate:
  - `scripts/check_openapi_governance_bundle_integrity.sh`
- Emergency exceptions are time-bounded via waiver file policy.

## Milestones

1. M1 (bootstrap):
- Contracts exist for both services.
- Core health and high-frequency chat/memory session paths are present.

2. M2 (adoption):
- OpenAPI path count reaches at least 30% of baseline endpoint count.
- Contract updates are required for backend endpoint additions.

3. M2.5 (required floor):
- CI enforces minimum coverage floor (currently 60% for both backends).
- Emergency exceptions require explicit waiver metadata and expiration.
- Waiver lifetime is policy-bounded (default max: 24h).

4. M3 (enforcement):
- OpenAPI path count reaches at least 60% of baseline endpoint count.
- CI threshold is raised to 60% (or above) as default policy.
- Method-level OpenAPI coverage reaches and maintains high threshold (current: 95%).
- Structural quality checks are enforced (summary/path-parameter/response quality).
- Semantic checks are enforced (operationId completeness/uniqueness + schema attachment).
- Semantic threshold (current): 100% on operationId and JSON schema attachment metrics.

## Notes

- Baseline endpoint counts come from:
  - `.github/api-path-baseline.txt`
- Coverage is tracked by:
  - `scripts/check_openapi_contract_assembly.sh`
  - `scripts/report_openapi_fragment_owners.sh`
  - `scripts/check_openapi_ownership_map_consistency.sh`
  - `scripts/check_openapi_owner_report_schema.sh`
  - `scripts/generate_openapi_ownership_drift_trend.sh`
  - `scripts/generate_openapi_governance_bundle.sh`
  - `scripts/check_openapi_governance_bundle_integrity.sh`
  - `scripts/generate_openapi_contract_change_summary.sh`
  - `scripts/check_openapi_contract_advisory.sh`
  - `scripts/check_openapi_contract_gate.sh`
  - `scripts/check_openapi_method_contract_gate.sh`
  - `scripts/check_openapi_contract_quality_gate.sh`
  - `scripts/check_openapi_contract_semantic_gate.sh`
