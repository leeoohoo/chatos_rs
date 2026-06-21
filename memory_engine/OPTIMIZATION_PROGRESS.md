# Optimization Progress

## Status Board

| Phase | Status | Scope | Notes |
| --- | --- | --- | --- |
| Phase 0 | Done | Add minimal safety net | Added unit tests around shared AI pipeline primitives in `backend/src/services/ai_pipeline/mod.rs` |
| Phase 1 | Done | Extract shared AI summarization pipeline | Introduced `backend/src/services/ai_pipeline/mod.rs` and migrated `summary.rs` + `subject_memory.rs` to use it |
| Phase 2 | In Progress | Split oversized service/frontend/SDK files | `backend/src/services/summary.rs`, `backend/src/services/subject_memory.rs`, `sdk/src/client.rs`, `sdk/src/models.rs`, `frontend/src/App.tsx`, `backend/src/api/sdk_api.rs`, `backend/src/api/mod.rs`, `backend/src/repositories/control_plane.rs`, `backend/src/repositories/summaries.rs`, `backend/src/repositories/records.rs`, `backend/src/repositories/subject_memories.rs`, `backend/src/repositories/threads.rs`, `backend/src/ai/mod.rs`, `backend/src/services/summary/thread_repair.rs`, `backend/src/services/subject_memory/job.rs`, `backend/src/jobs/summary_jobs.rs`, `frontend/src/app/sections/DataSection.tsx`, `backend/src/services/ai_pipeline/mod.rs`, `backend/src/repositories/control_plane/job_runs.rs`, `backend/src/services/context.rs`, `backend/src/services/summary/thread_repair/summary.rs`, and `frontend/src/app/hooks/useCatalogResources.ts` have all been decomposed; the remaining hotspots are now mostly medium-sized modules, so the next pass can reasonably choose between closing Phase 2 and starting Phase 3 aggregator cleanup |
| Phase 3 | In Progress | Split route and repository aggregators | Started with `backend/src/repositories/sources.rs`, `backend/src/api/threads_api.rs`, `backend/src/repositories/summaries/queries.rs`, `backend/src/api/admin_api.rs`, `backend/src/api/sdk_api/requests.rs`, and `backend/src/services/summary/rollup.rs`, all of which have now been decomposed into focused submodules |
| Phase 4 | Pending | Shared contracts | Not started |

## Completed Work

### Phase 0

- Added focused unit tests for:
  - token estimation floor
  - oversized single-item splitting behavior
  - overflow error classification
  - AI input assembly
- Reused the backend's existing unit-test setup instead of introducing a new test harness

Verification:

- `cargo +stable test --locked` in `backend/`

### Phase 1

- Added shared module:
  - `backend/src/services/ai_pipeline/mod.rs`
- Moved duplicated logic into the shared pipeline:
  - token estimation
  - oversized text splitting
  - chunk partitioning
  - overflow retry loop
  - chunk merge loop
  - AI input formatting
- Migrated these call sites to the shared implementation:
  - thread summary flow in `backend/src/services/summary.rs`
  - thread repair summary flow in `backend/src/services/summary.rs`
  - summary rollup flow in `backend/src/services/summary.rs`
  - subject-memory L0 generation in `backend/src/services/subject_memory.rs`
  - subject-memory rollup generation in `backend/src/services/subject_memory.rs`

Verification:

- `cargo +stable test --locked` in `backend/`
- `cargo +stable check --locked` in `backend/`

## Current Risks / Follow-up

- The project has now transitioned into Phase 3 aggregator cleanup; the next high-yield targets are the remaining medium-sized orchestration modules such as `backend/src/services/summary/thread_summary.rs` and `frontend/src/app/sections/RunsSection.tsx`.

## Phase 2 Progress

### Completed in this phase

- Split former `backend/src/services/summary.rs` into:
  - `backend/src/services/summary/mod.rs`
  - `backend/src/services/summary/settings.rs`
  - `backend/src/services/summary/render.rs`
  - `backend/src/services/summary/selectors.rs`
  - `backend/src/services/summary/builders.rs`
  - `backend/src/services/summary/thread_summary.rs`
  - `backend/src/services/summary/thread_repair.rs`
  - `backend/src/services/summary/rollup.rs`
- Preserved the original external call surface:
  - `crate::services::summary::run_thread_summary`
  - `crate::services::summary::run_thread_repair_summary`
  - `crate::services::summary::run_thread_repair_scope`
  - `crate::services::summary::get_thread_repair_scope_status`
  - `crate::services::summary::run_thread_rollup`
  - `crate::services::summary::thread_has_rollup_work`
  - `crate::services::summary::default_rollup_settings`
- Split former `backend/src/services/subject_memory.rs` into:
  - `backend/src/services/subject_memory/mod.rs`
  - `backend/src/services/subject_memory/settings.rs`
  - `backend/src/services/subject_memory/render.rs`
  - `backend/src/services/subject_memory/selectors.rs`
  - `backend/src/services/subject_memory/builders.rs`
  - `backend/src/services/subject_memory/job.rs`
  - `backend/src/services/subject_memory/scopes.rs`
- Preserved the original external call surface:
  - `crate::services::subject_memory::run_subject_memory_job`
  - `crate::services::subject_memory::run_registered_subject_memory_scopes`
  - `crate::services::subject_memory::run_registered_subject_memory_scopes_due`
- Split former `sdk/src/client.rs` into:
  - `sdk/src/client/mod.rs`
  - `sdk/src/client/transport.rs`
  - `sdk/src/client/admin.rs`
  - `sdk/src/client/threads.rs`
  - `sdk/src/client/records.rs`
  - `sdk/src/client/context.rs`
  - `sdk/src/client/snapshots.rs`
  - `sdk/src/client/summaries.rs`
  - `sdk/src/client/subject_memories.rs`
  - `sdk/src/client/jobs.rs`
- Preserved the original external SDK surface:
  - `memory_engine_sdk::MemoryEngineClient`
  - existing client constructors and request methods remain available from the same public type
- Split former `frontend/src/App.tsx` into:
  - `frontend/src/app/types.ts`
  - `frontend/src/app/constants.ts`
  - `frontend/src/app/utils.ts`
  - `frontend/src/app/components/PolicyEditorCard.tsx`
  - `frontend/src/app/sections/DashboardSection.tsx`
  - `frontend/src/app/sections/DataSection.tsx`
  - `frontend/src/app/sections/SourcesSection.tsx`
  - `frontend/src/app/sections/ModelsSection.tsx`
  - `frontend/src/app/sections/PoliciesSection.tsx`
  - `frontend/src/app/sections/RunsSection.tsx`
  - `frontend/src/app/modals/SourceModal.tsx`
  - `frontend/src/app/modals/ModelModal.tsx`
  - `frontend/src/app/modals/RotatedSecretModal.tsx`
- Kept `frontend/src/App.tsx` as the main composition layer and reduced it from 2039 lines to 731 lines
- Preserved the original admin-console behavior while separating:
  - shared formatting/helpers
  - policy editor UI
  - per-tab page sections
  - modal dialogs
- Split former `backend/src/api/sdk_api.rs` into:
  - `backend/src/api/sdk_api/mod.rs`
  - `backend/src/api/sdk_api/auth.rs`
  - `backend/src/api/sdk_api/requests.rs`
  - `backend/src/api/sdk_api/threads.rs`
  - `backend/src/api/sdk_api/records.rs`
  - `backend/src/api/sdk_api/context.rs`
  - `backend/src/api/sdk_api/snapshots.rs`
  - `backend/src/api/sdk_api/summaries.rs`
  - `backend/src/api/sdk_api/subject_memories.rs`
  - `backend/src/api/sdk_api/jobs.rs`
- Preserved the original SDK route handler surface used by `backend/src/api/mod.rs`
- Separated SDK auth extraction, SDK-only request DTOs, and handler groups by domain:
  - threads
  - records
  - context
  - snapshots
  - summaries
  - subject memories
  - scheduler / repair jobs
- Split former `backend/src/api/mod.rs` route registry into:
  - `backend/src/api/router/mod.rs`
  - `backend/src/api/router/admin.rs`
  - `backend/src/api/router/sdk.rs`
  - `backend/src/api/router/core.rs`
- Kept `backend/src/api/mod.rs` as the top-level composition entry and preserved all existing route paths
- Separated route registration concerns into:
  - admin APIs
  - SDK APIs
  - core/internal platform APIs
- Split former `backend/src/repositories/control_plane.rs` into:
  - `backend/src/repositories/control_plane/mod.rs`
  - `backend/src/repositories/control_plane/common.rs`
  - `backend/src/repositories/control_plane/model_profiles.rs`
  - `backend/src/repositories/control_plane/job_policies.rs`
  - `backend/src/repositories/control_plane/job_runs.rs`
- Preserved the original repository call surface used by:
  - `backend/src/api/admin_api.rs`
  - `backend/src/api/jobs_api.rs`
  - `backend/src/api/sdk_api/jobs.rs`
  - `backend/src/services/control_plane.rs`
  - `backend/src/jobs/summary_jobs.rs`
  - `backend/src/jobs/worker.rs`
- Separated control-plane repository concerns into:
  - model profile persistence
  - job policy persistence / normalization
  - job run lifecycle / stats / stale-run cleanup
- Split former `sdk/src/models.rs` into:
  - `sdk/src/models/mod.rs`
  - `sdk/src/models/common.rs`
  - `sdk/src/models/context.rs`
  - `sdk/src/models/records.rs`
  - `sdk/src/models/threads.rs`
  - `sdk/src/models/snapshots.rs`
  - `sdk/src/models/summaries.rs`
  - `sdk/src/models/subject_memories.rs`
  - `sdk/src/models/admin.rs`
- Preserved the original SDK export surface re-exported by `sdk/src/lib.rs`
- Grouped SDK models by responsibility to align with existing client modules:
  - shared list wrapper
  - context composition contracts
  - threads / records / snapshots
  - summaries / subject memories
  - admin / source / job control-plane models
- Split former `backend/src/repositories/summaries.rs` into:
  - `backend/src/repositories/summaries/mod.rs`
  - `backend/src/repositories/summaries/common.rs`
  - `backend/src/repositories/summaries/queries.rs`
  - `backend/src/repositories/summaries/writes.rs`
  - `backend/src/repositories/summaries/status.rs`
- Preserved the original repository call surface used by:
  - `backend/src/services/summary/`
  - `backend/src/services/subject_memory/`
  - `backend/src/jobs/summary_jobs.rs`
  - `backend/src/api/summaries_api.rs`
  - `backend/src/api/sdk_api/summaries.rs`
  - `backend/src/api/sdk_api/subject_memories.rs`
- Separated summary repository concerns into:
  - shared collection / cursor helpers
  - read/query flows
  - creation / upsert / delete flows
  - rollup and subject-memory status updates
- Split former `backend/src/repositories/records.rs` into:
  - `backend/src/repositories/records/mod.rs`
  - `backend/src/repositories/records/common.rs`
  - `backend/src/repositories/records/queries.rs`
  - `backend/src/repositories/records/writes.rs`
  - `backend/src/repositories/records/status.rs`
- Preserved the original repository call surface used by:
  - `backend/src/services/summary/`
  - `backend/src/api/records_api.rs`
  - `backend/src/api/threads_api.rs`
  - `backend/src/api/sdk_api/records.rs`
  - `backend/src/repositories/summaries/`
- Separated record repository concerns into:
  - shared collection / filter / cursor helpers
  - read/query flows
  - batch upsert and delete flows
  - summary-status mutation flows
- Split former `backend/src/repositories/subject_memories.rs` into:
  - `backend/src/repositories/subject_memories/mod.rs`
  - `backend/src/repositories/subject_memories/common.rs`
  - `backend/src/repositories/subject_memories/queries.rs`
  - `backend/src/repositories/subject_memories/writes.rs`
  - `backend/src/repositories/subject_memories/status.rs`
- Preserved the original repository call surface used by:
  - `backend/src/services/subject_memory/`
  - `backend/src/api/subject_memories_api.rs`
  - `backend/src/api/sdk_api/subject_memories.rs`
  - `backend/src/services/context.rs`
- Separated subject-memory repository concerns into:
  - shared collection / filter / upsert helpers
  - read/query flows
  - generated and direct upsert flows
  - rollup status mutation flow
- Follow-up cleanup completed in the same pass:
  - rewired `backend/src/services/context.rs` to consume shared repository queries instead of duplicating MongoDB access logic
  - removed the previously noted repository dead-code warnings by reusing:
    - `records::list_recent_records`
    - `summaries::list_latest_thread_summaries`
    - `summaries::list_latest_thread_summaries_at_level`
    - `subject_memories::list_subject_memories_by_subject_ids`
- Split former `backend/src/ai/mod.rs` into:
  - `backend/src/ai/mod.rs`
  - `backend/src/ai/client.rs`
  - `backend/src/ai/protocol.rs`
  - `backend/src/ai/parsing.rs`
  - `backend/src/ai/tests.rs`
- Preserved the original AI client entry point:
  - `crate::ai::AiClient`
- Separated AI module concerns into:
  - client lifecycle and outbound request execution
  - provider / endpoint / payload rules
  - response parsing and log-safe truncation
  - focused unit tests for provider rules and parser behavior
- Split former `backend/src/services/summary/thread_repair.rs` into:
  - `backend/src/services/summary/thread_repair/mod.rs`
  - `backend/src/services/summary/thread_repair/common.rs`
  - `backend/src/services/summary/thread_repair/summary.rs`
  - `backend/src/services/summary/thread_repair/scope.rs`
- Preserved the original summary-service call surface:
  - `crate::services::summary::run_thread_repair_summary`
  - `crate::services::summary::run_thread_repair_scope`
  - `crate::services::summary::get_thread_repair_scope_status`
- Separated thread-repair concerns into:
  - shared preparation / scope scanning helpers
  - single-thread repair job orchestration
  - scope execution and scope status calculation
- Split former `backend/src/services/subject_memory/job.rs` into:
  - `backend/src/services/subject_memory/job/mod.rs`
  - `backend/src/services/subject_memory/job/common.rs`
  - `backend/src/services/subject_memory/job/level0.rs`
  - `backend/src/services/subject_memory/job/rollup.rs`
  - `backend/src/services/subject_memory/job/runner.rs`
- Preserved the original subject-memory service call surface:
  - `crate::services::subject_memory::run_subject_memory_job`
  - internal scope runner integration continues through `run_subject_memory_job_internal`
- Separated subject-memory job concerns into:
  - shared preparation and job-run metadata helpers
  - level0 generation / reuse path
  - rollup generation / reuse path
  - top-level orchestration and final job-run completion flow
- Split former `backend/src/jobs/summary_jobs.rs` into:
  - `backend/src/jobs/summary_jobs/mod.rs`
  - `backend/src/jobs/summary_jobs/common.rs`
  - `backend/src/jobs/summary_jobs/summaries.rs`
  - `backend/src/jobs/summary_jobs/rollups.rs`
- Preserved the original scheduler/API call surface:
  - `crate::jobs::summary_jobs::run_pending_thread_summaries`
  - `crate::jobs::summary_jobs::run_pending_thread_summaries_due`
  - `crate::jobs::summary_jobs::run_pending_thread_summaries_with_limit`
  - `crate::jobs::summary_jobs::run_pending_thread_rollups`
  - `crate::jobs::summary_jobs::run_pending_thread_rollups_due`
- Separated scheduler concerns into:
  - shared scheduler job-run helpers
  - pending thread summary execution
  - pending rollup execution
- Further reduced `frontend/src/App.tsx` by extracting orchestration hooks:
  - `frontend/src/app/hooks/useConsoleResources.ts`
  - `frontend/src/app/hooks/useThreadExplorer.ts`
- Kept `frontend/src/App.tsx` as the UI composition layer while moving out:
  - resource loading and refresh flows
  - source / model / policy / run state and handlers
  - thread explorer state, filters, and detail loading
- Preserved existing frontend behavior and existing section/modal component boundaries
- Split the extracted `frontend/src/app/hooks/useConsoleResources.ts` orchestration further into:
  - `frontend/src/app/hooks/useConsoleResources.ts`
  - `frontend/src/app/hooks/useCatalogResources.ts`
  - `frontend/src/app/hooks/useRunManagement.ts`
- Separated frontend console orchestration concerns into:
  - sources / models / policies / modal form management
  - job-run filters / stats / thread-name hydration
  - top-level dashboard aggregation and initial page bootstrap
- Split former `backend/src/repositories/threads.rs` into:
  - `backend/src/repositories/threads/mod.rs`
  - `backend/src/repositories/threads/common.rs`
  - `backend/src/repositories/threads/queries.rs`
  - `backend/src/repositories/threads/writes.rs`
- Preserved the original repository call surface used by:
  - `backend/src/services/context.rs`
  - `backend/src/services/summary/`
  - `backend/src/jobs/summary_jobs/`
  - `backend/src/api/threads_api.rs`
  - `backend/src/api/sdk_api/threads.rs`
- Separated thread repository concerns into:
  - shared collections / cursor helpers / filter normalization
  - lookup and list queries
  - thread upsert lifecycle
- Split former `frontend/src/app/sections/DataSection.tsx` further into:
  - `frontend/src/app/sections/DataSection.tsx`
  - `frontend/src/app/sections/data/types.ts`
  - `frontend/src/app/sections/data/DataFiltersCard.tsx`
  - `frontend/src/app/sections/data/ThreadWorkspace.tsx`
  - `frontend/src/app/sections/data/columns.tsx`
- Kept `frontend/src/app/sections/DataSection.tsx` as the page composition layer while moving out:
  - filter form rendering
  - thread / record / summary / subject-memory table column definitions
  - selected-thread detail card rendering
- Preserved the existing data-console behavior and prop surface consumed by `frontend/src/App.tsx`
- Split former `backend/src/services/ai_pipeline/mod.rs` into:
  - `backend/src/services/ai_pipeline/mod.rs`
  - `backend/src/services/ai_pipeline/types.rs`
  - `backend/src/services/ai_pipeline/chunking.rs`
  - `backend/src/services/ai_pipeline/input.rs`
  - `backend/src/services/ai_pipeline/overflow.rs`
  - `backend/src/services/ai_pipeline/pipeline.rs`
  - `backend/src/services/ai_pipeline/tests.rs`
- Preserved the original shared AI pipeline surface:
  - `crate::services::ai_pipeline::summarize_texts_with_split`
  - `crate::services::ai_pipeline::estimate_tokens_text`
  - `crate::services::ai_pipeline::is_context_overflow_error`
  - `crate::services::ai_pipeline::MIN_TOKEN_LIMIT`
  - `crate::services::ai_pipeline::SummarizeTextsOptions`
  - `crate::services::ai_pipeline::SummaryBuildResult`
- Separated AI pipeline concerns into:
  - reusable option/result types and limits
  - token estimation and chunk splitting
  - AI input assembly
  - overflow detection
  - summarize / merge execution flow
  - focused unit tests
- Split former `backend/src/repositories/control_plane/job_runs.rs` into:
  - `backend/src/repositories/control_plane/job_runs/mod.rs`
  - `backend/src/repositories/control_plane/job_runs/lifecycle.rs`
  - `backend/src/repositories/control_plane/job_runs/stale.rs`
  - `backend/src/repositories/control_plane/job_runs/queries.rs`
  - `backend/src/repositories/control_plane/job_runs/stats.rs`
- Preserved the original control-plane job-run repository surface:
  - `crate::repositories::control_plane::create_job_run`
  - `crate::repositories::control_plane::finish_job_run`
  - `crate::repositories::control_plane::list_job_runs`
  - `crate::repositories::control_plane::has_recent_job_run`
  - `crate::repositories::control_plane::job_run_stats`
- Separated job-run repository concerns into:
  - run creation / finish lifecycle updates
  - stale-running-job timeout cleanup
  - filtered list / recent-run existence queries
  - aggregate stats building
- Split former `backend/src/services/context.rs` into:
  - `backend/src/services/context/mod.rs`
  - `backend/src/services/context/policy.rs`
  - `backend/src/services/context/blocks.rs`
  - `backend/src/services/context/tests.rs`
- Preserved the original context-service entry point:
  - `crate::services::context::compose_context`
- Separated context composition concerns into:
  - request-policy normalization
  - thread summary / subject memory block assembly
  - subject-id collection and display formatting helpers
  - focused unit tests for block formatting and subject-id normalization
- Split former `backend/src/services/summary/thread_repair/summary.rs` further into:
  - `backend/src/services/summary/thread_repair/summary/mod.rs`
  - `backend/src/services/summary/thread_repair/summary/runner.rs`
  - `backend/src/services/summary/thread_repair/summary/metadata.rs`
- Preserved the original thread-repair entry point:
  - `crate::services::summary::run_thread_repair_summary`
- Separated thread-repair summary concerns into:
  - request entry / detached job spawning
  - repair-summary job execution
  - response builders and job-run metadata payload helpers
- Split former `frontend/src/app/hooks/useCatalogResources.ts` further into:
  - `frontend/src/app/hooks/useCatalogResources.ts`
  - `frontend/src/app/hooks/catalog/useCatalogState.ts`
  - `frontend/src/app/hooks/catalog/useCatalogLoaders.ts`
  - `frontend/src/app/hooks/catalog/useCatalogActions.ts`
  - `frontend/src/app/hooks/catalog/types.ts`
- Preserved the original catalog-resource hook surface consumed by `frontend/src/app/hooks/useConsoleResources.ts` and `frontend/src/App.tsx`
- Separated catalog hook concerns into:
  - catalog state, memoized options, and policy-view selection
  - source / model / policy loader functions
  - source / model modal actions and policy save handlers

Verification:

- `cargo +stable check --locked` in `backend/`
- `cargo +stable test --locked` in `backend/`
- `cargo +stable check --locked` in `sdk/`
- `cargo +stable test --locked` in `sdk/`
- `npm run type-check` in `frontend/`
- `npm run build` in `frontend/`
- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/ai_pipeline/mod.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/ai_pipeline/mod.rs`
- `cargo +stable check --locked` in `backend/` after splitting `backend/src/repositories/control_plane/job_runs.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/repositories/control_plane/job_runs.rs`
- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/context.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/context.rs`
- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/summary/thread_repair/summary.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/summary/thread_repair/summary.rs`
- `npm run type-check` in `frontend/` after splitting `frontend/src/app/sections/DataSection.tsx`
- `npm run build` in `frontend/` after splitting `frontend/src/app/sections/DataSection.tsx`
- `npm run type-check` in `frontend/` after splitting `frontend/src/app/hooks/useCatalogResources.ts`
- `npm run build` in `frontend/` after splitting `frontend/src/app/hooks/useCatalogResources.ts`

## Next Step

Continue by choosing between one final Phase 2 medium-file split and Phase 3 aggregator cleanup. Current suggested order:

```text
1. start Phase 3 aggregator cleanup
2. reserve any further Phase 2-style splits for files that become change bottlenecks in practice
3. reserve Phase 4 for contract consolidation only after module boundaries stabilize
```

## Phase 3 Progress

### Completed in this phase

- Split former `backend/src/repositories/sources.rs` into:
  - `backend/src/repositories/sources/mod.rs`
  - `backend/src/repositories/sources/common.rs`
  - `backend/src/repositories/sources/queries.rs`
  - `backend/src/repositories/sources/writes.rs`
  - `backend/src/repositories/sources/secrets.rs`
- Preserved the original source repository surface:
  - `crate::repositories::sources::upsert_source`
  - `crate::repositories::sources::list_sources`
  - `crate::repositories::sources::rotate_source_secret`
  - `crate::repositories::sources::verify_source_secret`
  - `crate::repositories::sources::is_source_active`
  - `crate::repositories::sources::is_retired_source_id`
- Separated source repository concerns into:
  - shared collection / filter / normalization / hashing helpers
  - source listing and active/secret verification queries
  - source upsert lifecycle
  - source secret rotation flow

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/repositories/sources.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/repositories/sources.rs`

- Split former `backend/src/api/threads_api.rs` into:
  - `backend/src/api/threads_api/mod.rs`
  - `backend/src/api/threads_api/error.rs`
  - `backend/src/api/threads_api/queries.rs`
  - `backend/src/api/threads_api/threads.rs`
  - `backend/src/api/threads_api/records.rs`
- Preserved the original thread API handler surface used by `backend/src/api/router/core.rs`:
  - `crate::api::threads_api::upsert_thread`
  - `crate::api::threads_api::get_thread`
  - `crate::api::threads_api::list_threads_query`
  - `crate::api::threads_api::list_threads_by_label`
  - `crate::api::threads_api::batch_sync_records`
  - `crate::api::threads_api::list_records`
  - `crate::api::threads_api::delete_records`
  - `crate::api::threads_api::count_records`
- Separated thread API concerns into:
  - shared internal error mapping
  - query DTO definitions
  - thread-oriented handlers
  - thread-record handlers

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/api/threads_api.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/api/threads_api.rs`

- Split former `backend/src/repositories/summaries/queries.rs` into:
  - `backend/src/repositories/summaries/queries/mod.rs`
  - `backend/src/repositories/summaries/queries/thread.rs`
  - `backend/src/repositories/summaries/queries/labels.rs`
  - `backend/src/repositories/summaries/queries/rollups.rs`
- Preserved the original summary-query repository surface:
  - `crate::repositories::summaries::list_latest_thread_summaries`
  - `crate::repositories::summaries::list_latest_thread_summaries_at_level`
  - `crate::repositories::summaries::list_latest_thread_summaries_by_type`
  - `crate::repositories::summaries::list_thread_summaries`
  - `crate::repositories::summaries::list_summaries_by_thread_label`
  - `crate::repositories::summaries::find_summary_by_source_digest`
  - `crate::repositories::summaries::list_pending_summaries_by_level`
  - `crate::repositories::summaries::list_threads_with_pending_rollups`
- Separated summary-query concerns into:
  - thread-scoped latest/list queries
  - thread-label summary lookups
  - rollup candidate and digest lookup queries

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/repositories/summaries/queries.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/repositories/summaries/queries.rs`

- Split former `backend/src/api/admin_api.rs` into:
  - `backend/src/api/admin_api/mod.rs`
  - `backend/src/api/admin_api/error.rs`
  - `backend/src/api/admin_api/queries.rs`
  - `backend/src/api/admin_api/model_profiles.rs`
  - `backend/src/api/admin_api/policies.rs`
  - `backend/src/api/admin_api/job_runs.rs`
- Preserved the original admin API handler surface used by `backend/src/api/router/admin.rs`:
  - `crate::api::admin_api::list_model_profiles`
  - `crate::api::admin_api::create_model_profile`
  - `crate::api::admin_api::update_model_profile`
  - `crate::api::admin_api::delete_model_profile`
  - `crate::api::admin_api::list_job_policies`
  - `crate::api::admin_api::get_job_policy`
  - `crate::api::admin_api::upsert_job_policy`
  - `crate::api::admin_api::list_job_runs`
  - `crate::api::admin_api::job_run_stats`
- Separated admin API concerns into:
  - shared internal error mapping
  - admin query DTO definitions
  - model profile handlers
  - job policy handlers
  - job run handlers

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/api/admin_api.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/api/admin_api.rs`

- Split former `backend/src/api/sdk_api/requests.rs` into:
  - `backend/src/api/sdk_api/requests/mod.rs`
  - `backend/src/api/sdk_api/requests/threads.rs`
  - `backend/src/api/sdk_api/requests/records.rs`
  - `backend/src/api/sdk_api/requests/context.rs`
  - `backend/src/api/sdk_api/requests/summaries.rs`
  - `backend/src/api/sdk_api/requests/jobs.rs`
  - `backend/src/api/sdk_api/requests/subject_memories.rs`
  - `backend/src/api/sdk_api/requests/snapshots.rs`
- Preserved the original SDK request DTO surface consumed by `backend/src/api/sdk_api/*`
- Separated SDK request contracts into:
  - thread DTOs
  - record DTOs
  - context DTOs
  - summary DTOs
  - job trigger DTOs
  - subject-memory DTOs
  - snapshot DTOs

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/api/sdk_api/requests.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/api/sdk_api/requests.rs`

- Split former `backend/src/services/summary/rollup.rs` further into:
  - `backend/src/services/summary/rollup/mod.rs`
  - `backend/src/services/summary/rollup/settings.rs`
  - `backend/src/services/summary/rollup/job.rs`
  - `backend/src/services/summary/rollup/execution.rs`
- Preserved the original summary rollup surface:
  - `crate::services::summary::default_rollup_settings`
  - `crate::services::summary::run_thread_rollup`
  - `crate::services::summary::thread_has_rollup_work`
- Separated rollup concerns into:
  - rollup default settings
  - rollup job-run lifecycle metadata and finish helpers
  - rollup batch execution and failure handling

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/summary/rollup.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/summary/rollup.rs`

- Split former `backend/src/services/summary/thread_summary.rs` further into:
  - `backend/src/services/summary/thread_summary/mod.rs`
  - `backend/src/services/summary/thread_summary/job.rs`
  - `backend/src/services/summary/thread_summary/execution.rs`
- Preserved the original thread-summary surface:
  - `crate::services::summary::run_thread_summary`
- Separated thread-summary concerns into:
  - thread-summary job-run creation, completion helpers, and metadata builders
  - thread-summary batch execution, summary creation, and rollback/failure handling

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/summary/thread_summary.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/summary/thread_summary.rs`

- Split former `frontend/src/app/sections/RunsSection.tsx` further into:
  - `frontend/src/app/sections/RunsSection.tsx`
  - `frontend/src/app/sections/runs/types.ts`
  - `frontend/src/app/sections/runs/RunFiltersCard.tsx`
  - `frontend/src/app/sections/runs/columns.tsx`
- Kept `frontend/src/app/sections/RunsSection.tsx` as the page composition layer while moving out:
  - job-run filter form rendering
  - job-run table column definitions and thread-name display wiring
- Preserved the existing run-console behavior and prop surface consumed by `frontend/src/App.tsx`

Verification:

- `npm run type-check` in `frontend/` after splitting `frontend/src/app/sections/RunsSection.tsx`
- `npm run build` in `frontend/` after splitting `frontend/src/app/sections/RunsSection.tsx`

- Split former `frontend/src/app/utils.ts` into:
  - `frontend/src/app/utils.ts`
  - `frontend/src/app/utils/common.ts`
  - `frontend/src/app/utils/display.ts`
  - `frontend/src/app/utils/thread.ts`
  - `frontend/src/app/utils/record.ts`
  - `frontend/src/app/utils/forms.ts`
- Preserved the original utility import surface by keeping `frontend/src/app/utils.ts` as a compatibility re-export layer
- Separated frontend utility concerns into:
  - shared scalar and object helpers
  - display formatting helpers
  - thread/job-run naming helpers
  - record tool-result formatting helpers
  - modal form initialization and API payload builders

Verification:

- `npm run type-check` in `frontend/` after splitting `frontend/src/app/utils.ts`
- `npm run build` in `frontend/` after splitting `frontend/src/app/utils.ts`

- Split former `frontend/src/types.ts` into:
  - `frontend/src/types.ts`
  - `frontend/src/types/models.ts`
  - `frontend/src/types/sources.ts`
  - `frontend/src/types/threads.ts`
  - `frontend/src/types/summaries.ts`
  - `frontend/src/types/jobs.ts`
- Preserved the original frontend type import surface by keeping `frontend/src/types.ts` as a compatibility re-export layer
- Separated frontend contract concerns into:
  - model profile types
  - source and source-secret types
  - thread and record query/data types
  - summary and subject-memory types
  - job policy and job-run types

Verification:

- `npm run type-check` in `frontend/` after splitting `frontend/src/types.ts`
- `npm run build` in `frontend/` after splitting `frontend/src/types.ts`

- Split former `backend/src/ai/client.rs` into:
  - `backend/src/ai/client/mod.rs`
  - `backend/src/ai/client/config.rs`
  - `backend/src/ai/client/request.rs`
  - `backend/src/ai/client/responses.rs`
- Preserved the original AI client entry point:
  - `crate::ai::AiClient`
- Separated AI client concerns into:
  - profile/config normalization and client construction
  - top-level summarize request orchestration and timeout/log handling
  - shared outbound JSON request execution
  - chat-completions / responses request builders and response validation

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/ai/client.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/ai/client.rs`

- Split former `sdk/src/client/summaries.rs` into:
  - `sdk/src/client/summaries/mod.rs`
  - `sdk/src/client/summaries/thread.rs`
  - `sdk/src/client/summaries/labels.rs`
  - `sdk/src/client/summaries/repair.rs`
  - `sdk/src/client/summaries/triggers.rs`
- Preserved the original SDK client summary surface:
  - `MemoryEngineClient::list_thread_summaries`
  - `MemoryEngineClient::delete_thread_summary`
  - `MemoryEngineClient::list_summaries_by_thread_label`
  - `MemoryEngineClient::list_summaries_by_thread_label_system`
  - `MemoryEngineClient::run_thread_summary`
  - `MemoryEngineClient::run_thread_repair_summary`
  - `MemoryEngineClient::run_thread_repair_scope`
  - `MemoryEngineClient::get_thread_repair_scope_status`
- Separated SDK summary-client concerns into:
  - thread summary list/delete methods
  - thread-label summary query methods
  - repair-scope APIs
  - thread summary / repair trigger methods

Verification:

- `cargo +stable check --locked` in `sdk/` after splitting `sdk/src/client/summaries.rs`
- `cargo +stable test --locked` in `sdk/` after splitting `sdk/src/client/summaries.rs`

- Split former `frontend/src/api.ts` into:
  - `frontend/src/api.ts`
  - `frontend/src/api/client.ts`
  - `frontend/src/api/admin.ts`
  - `frontend/src/api/threads.ts`
- Preserved the original frontend API entry point by keeping `frontend/src/api.ts` as a compatibility composition layer
- Separated frontend API concerns into:
  - shared axios client construction
  - admin/source/model/policy/job-run APIs
  - thread/record/summary/subject-memory APIs

Verification:

- `npm run type-check` in `frontend/` after splitting `frontend/src/api.ts`
- `npm run build` in `frontend/` after splitting `frontend/src/api.ts`

- Split former `backend/src/services/subject_memory/job/common.rs` further into:
  - `backend/src/services/subject_memory/job/common/mod.rs`
  - `backend/src/services/subject_memory/job/common/state.rs`
  - `backend/src/services/subject_memory/job/common/prepare.rs`
  - `backend/src/services/subject_memory/job/common/job_run.rs`
- Preserved the original internal job-common call surface consumed by:
  - `backend/src/services/subject_memory/job/runner.rs`
  - `backend/src/services/subject_memory/job/level0.rs`
  - `backend/src/services/subject_memory/job/rollup.rs`
- Separated subject-memory job common concerns into:
  - mutable progress state and counters
  - summary/rollup preparation and noop response helpers
  - job-run creation, finish helpers, and success/failure metadata builders

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/subject_memory/job/common.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/subject_memory/job/common.rs`

- Split former `frontend/src/app/components/PolicyEditorCard.tsx` into:
  - `frontend/src/app/components/PolicyEditorCard.tsx`
  - `frontend/src/app/components/policy/types.ts`
  - `frontend/src/app/components/policy/PolicySummary.tsx`
  - `frontend/src/app/components/policy/PolicyFields.tsx`
- Kept `frontend/src/app/components/PolicyEditorCard.tsx` as the orchestration layer while moving out:
  - policy description / summary header rendering
  - policy form field rendering
- Preserved the existing prop surface consumed by `frontend/src/App.tsx`

Verification:

- `npm run type-check` in `frontend/` after splitting `frontend/src/app/components/PolicyEditorCard.tsx`
- `npm run build` in `frontend/` after splitting `frontend/src/app/components/PolicyEditorCard.tsx`

- Split former `backend/src/services/ai_pipeline/pipeline.rs` into:
  - `backend/src/services/ai_pipeline/pipeline/mod.rs`
  - `backend/src/services/ai_pipeline/pipeline/retry.rs`
  - `backend/src/services/ai_pipeline/pipeline/execution.rs`
- Preserved the original shared AI pipeline entry point:
  - `crate::services::ai_pipeline::summarize_texts_with_split`
- Separated AI pipeline execution concerns into:
  - overflow-retry orchestration and token-limit backoff
  - per-chunk summarize execution
  - chunk-summary merge execution

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/ai_pipeline/pipeline.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/ai_pipeline/pipeline.rs`

- Split former `frontend/src/app/hooks/catalog/useCatalogActions.ts` into:
  - `frontend/src/app/hooks/catalog/useCatalogActions.ts`
  - `frontend/src/app/hooks/catalog/actions/index.ts`
  - `frontend/src/app/hooks/catalog/actions/types.ts`
  - `frontend/src/app/hooks/catalog/actions/modal.ts`
  - `frontend/src/app/hooks/catalog/actions/source.ts`
  - `frontend/src/app/hooks/catalog/actions/model.ts`
  - `frontend/src/app/hooks/catalog/actions/policy.ts`
- Kept `frontend/src/app/hooks/catalog/useCatalogActions.ts` as the orchestration hook while moving out:
  - modal open/close actions
  - source submit/rotate actions
  - model submit/delete actions
  - policy save action
- Preserved the existing hook surface consumed by `frontend/src/app/hooks/useCatalogResources.ts`

Verification:

- `npm run type-check` in `frontend/` after splitting `frontend/src/app/hooks/catalog/useCatalogActions.ts`
- `npm run build` in `frontend/` after splitting `frontend/src/app/hooks/catalog/useCatalogActions.ts`

- Split former `backend/src/services/summary/thread_repair/common.rs` further into:
  - `backend/src/services/summary/thread_repair/common/mod.rs`
  - `backend/src/services/summary/thread_repair/common/filters.rs`
  - `backend/src/services/summary/thread_repair/common/prepare.rs`
  - `backend/src/services/summary/thread_repair/common/scope.rs`
- Preserved the original internal thread-repair common surface consumed by:
  - `backend/src/services/summary/thread_repair/scope.rs`
  - `backend/src/services/summary/thread_repair/summary/runner.rs`
  - `backend/src/services/summary/thread_repair/summary/metadata.rs`
- Separated thread-repair common concerns into:
  - thread-label and optional-filter normalization
  - repair-summary preparation and selection logging
  - scope thread listing, pending-thread scanning, and running-job counting

Verification:

- `cargo +stable check --locked` in `backend/` after splitting `backend/src/services/summary/thread_repair/common.rs`
- `cargo +stable test --locked` in `backend/` after splitting `backend/src/services/summary/thread_repair/common.rs`

- Split former `frontend/src/app/sections/data/columns.tsx` into:
  - `frontend/src/app/sections/data/columns/index.ts`
  - `frontend/src/app/sections/data/columns/recordCell.tsx`
  - `frontend/src/app/sections/data/columns/threadColumns.tsx`
  - `frontend/src/app/sections/data/columns/recordColumns.tsx`
  - `frontend/src/app/sections/data/columns/summaryColumns.tsx`
  - `frontend/src/app/sections/data/columns/subjectMemoryColumns.tsx`
- Preserved the existing column import surface consumed by `frontend/src/app/sections/data/ThreadWorkspace.tsx`
- Separated data-table presentation concerns into:
  - record content cell rendering
  - thread table columns
  - record table columns
  - summary table columns
  - subject-memory table columns

Verification:

- `npm run type-check` in `frontend/` after splitting `frontend/src/app/sections/data/columns.tsx`
- `npm run build` in `frontend/` after splitting `frontend/src/app/sections/data/columns.tsx`

- Split former `sdk/src/client/records.rs` into:
  - `sdk/src/client/records/mod.rs`
  - `sdk/src/client/records/thread.rs`
  - `sdk/src/client/records/lookup.rs`
  - `sdk/src/client/records/writes.rs`
- Preserved the original SDK client record surface:
  - `MemoryEngineClient::batch_sync_records`
  - `MemoryEngineClient::ingest_thread_records`
  - `MemoryEngineClient::delete_thread_records`
  - `MemoryEngineClient::delete_record`
  - `MemoryEngineClient::list_thread_records`
  - `MemoryEngineClient::count_thread_records`
  - `MemoryEngineClient::get_record`
- Separated SDK record-client concerns into:
  - thread-scoped batch sync, delete, list, and count methods
  - single-record lookup method
  - single-record delete method

Verification:

- `cargo +stable check --locked` in `sdk/` after splitting `sdk/src/client/records.rs`
- `cargo +stable test --locked` in `sdk/` after splitting `sdk/src/client/records.rs`

- Split former `sdk/src/client/admin.rs` into:
  - `sdk/src/client/admin/mod.rs`
  - `sdk/src/client/admin/model_profiles.rs`
  - `sdk/src/client/admin/job_policies.rs`
  - `sdk/src/client/admin/sources.rs`
  - `sdk/src/client/admin/auth.rs`
  - `sdk/src/client/admin/job_runs.rs`
- Preserved the original SDK admin-client surface:
  - model profile methods
  - job policy methods
  - source management methods
  - SDK auth-status method
  - job-run list/stats methods
- Separated SDK admin-client concerns into:
  - model profile CRUD
  - job policy get/upsert
  - source list/upsert/secret rotation
  - system-key auth status
  - job-run list and stats queries

Verification:

- `cargo +stable check --locked` in `sdk/` after splitting `sdk/src/client/admin.rs`
- `cargo +stable test --locked` in `sdk/` after splitting `sdk/src/client/admin.rs`
