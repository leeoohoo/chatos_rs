# Ownership Manifest

This folder stores the single-source ownership manifest used by:

- `scripts/report_openapi_fragment_owners.sh`
- `scripts/check_openapi_ownership_map_consistency.sh`
- `scripts/generate_openapi_ownership_drift_trend.sh`
- `scripts/generate_openapi_governance_bundle.sh`

## File

- `manifest.yaml`
- `owner-report.schema.json`
- `governance-bundle-index.schema.json`

## Schema

- `version`: manifest schema version
- `defaults.owner_hint`: fallback owner hint for unknown fragment files
- `defaults.owner_footprint`: fallback footprint (typically `fallback`)
- `fragments[]`:
  - `file`: fragment file path
  - `owner_hint`: reviewer hint text
  - `owner_footprint`: usually `mapped`
  - `codeowners[]`: expected owners in `.github/CODEOWNERS.openapi`
  - `ownership_map_rules[]` (optional):
    - `path_prefix`: path prefix in `OWNERSHIP_MAP.md`
    - `owner_scope`: expected scope text in `OWNERSHIP_MAP.md`

## Rule

- When adding/changing fragment domains, update:
  - `manifest.yaml`
  - `.github/api-contract/OWNERSHIP_MAP.md`
  - `.github/CODEOWNERS.openapi`
- CI verifies alignment via `scripts/check_openapi_ownership_map_consistency.sh`.
- JSON artifact schema is validated by `scripts/check_openapi_owner_report_schema.sh`.
- Consistency check emits machine-readable discrepancy artifact with severity:
  - `missing_fragment`
  - `missing_codeowner`
  - `scope_mismatch`
- Trend snapshot compares base/head drift count:
  - `scripts/generate_openapi_ownership_drift_trend.sh`
- Governance bundle consolidates owner report + discrepancy + trend:
  - `scripts/generate_openapi_governance_bundle.sh`
  - includes `artifact-index.json` and `GOVERNANCE_SUMMARY.md`
  - supports shared-artifact handoff to reduce CI recomputation:
    - `OPENAPI_GOVERNANCE_BUNDLE_REUSE_EXISTING=true`
    - `OPENAPI_CHANGE_SUMMARY_INPUT`
    - `OPENAPI_OWNER_REPORT_INPUT`
    - `OPENAPI_PR_COMMENT_DRAFT_INPUT`
    - `OPENAPI_OWNERSHIP_DISCREPANCY_INPUT`
    - `OPENAPI_OWNERSHIP_DRIFT_TREND_INPUT`
    - `OPENAPI_OWNERSHIP_DRIFT_TREND_JSON_INPUT`
- Governance bundle integrity is validated by:
  - `scripts/check_openapi_governance_bundle_integrity.sh`
