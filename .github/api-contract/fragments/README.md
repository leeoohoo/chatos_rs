# OpenAPI Fragments Workflow

This directory stores domain-split OpenAPI fragments to reduce merge conflicts.

## Layout

- `chat_app_server_rs/`
  - `_meta.yaml`: shared OpenAPI metadata (`openapi`, `info`, `servers`, `components`)
  - `NN-*.yaml`: domain path fragments (`paths`)
- `memory_server/`
  - same structure as above

## Commands

1. Validate assembly drift:
   - `bash scripts/check_openapi_contract_assembly.sh`
2. Rebuild assembled OpenAPI files from fragments:
   - `bash scripts/update_openapi_contract_assembly.sh`
3. Print owner hints for changed fragments:
   - `bash scripts/report_openapi_fragment_owners.sh`
   - JSON output: `bash scripts/report_openapi_fragment_owners.sh --json`
4. Generate markdown change summary:
   - `bash scripts/generate_openapi_contract_change_summary.sh`
5. Enforce owner footprint policy:
   - `bash scripts/check_openapi_fragment_owner_policy.sh`
6. Enforce OWNERSHIP_MAP and CODEOWNERS alignment:
   - `bash scripts/check_openapi_ownership_map_consistency.sh`
7. Validate owner-report JSON schema:
   - `bash scripts/check_openapi_owner_report_schema.sh`
8. Generate ownership drift trend snapshot:
   - `bash scripts/generate_openapi_ownership_drift_trend.sh`
9. Generate governance bundle (single entry):
   - `bash scripts/generate_openapi_governance_bundle.sh`
10. Validate governance bundle integrity:
   - `bash scripts/check_openapi_governance_bundle_integrity.sh`

## Rule

- Edit fragments first, then rebuild assembled files.
- CI enforces assembly parity before contract gates.
- CI emits owner-aware fragment report and uploads change-summary artifact.
- CI uploads machine-readable owner JSON summary and PR comment draft artifact.
- CI validates owner JSON artifact against schema.
- CI uploads ownership discrepancy JSON and base/head trend snapshot artifacts.
- CI uploads consolidated governance bundle artifact.
- CI validates governance bundle integrity before upload.
- Optional owner mapping file: `.github/CODEOWNERS.openapi`.
- Ownership source-of-truth manifest: `.github/api-contract/ownership/manifest.yaml`.
