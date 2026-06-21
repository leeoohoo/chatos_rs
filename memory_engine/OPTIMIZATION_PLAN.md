# Memory Engine Optimization Plan

## 1. Current Snapshot

This review focused on real source files only and ignored build artifacts such as `backend/target` and `frontend/node_modules`.

- Source file count: about 62 files
- Source line count: about 16,113 lines
- Largest source files:
  - `frontend/src/App.tsx`: 2039 lines
  - `backend/src/services/summary.rs`: 1897 lines
  - `sdk/src/client.rs`: 1542 lines
  - `backend/src/services/subject_memory.rs`: 1328 lines
  - `backend/src/api/sdk_api.rs`: 874 lines
  - `backend/src/repositories/control_plane.rs`: 753 lines
  - `sdk/src/models.rs`: 733 lines
  - `backend/src/ai/mod.rs`: 578 lines
  - `backend/src/repositories/summaries.rs`: 562 lines

## 2. Main Problems

### 2.1 Duplicate AI summarization pipeline logic

`backend/src/services/summary.rs` and `backend/src/services/subject_memory.rs` both contain their own versions of:

- token estimation
- text chunk splitting
- context overflow retry
- chunk summary merge
- AI input assembly
- generated text decoration

This is the clearest abstraction opportunity in the project. It is also the highest-return cleanup because the same behavior is implemented twice and will drift over time.

### 2.2 Large files with mixed responsibilities

Several files are doing orchestration, business rules, formatting, retry policy, repository coordination, and API mapping all together. The biggest examples are:

- `backend/src/services/summary.rs`
- `backend/src/services/subject_memory.rs`
- `backend/src/api/sdk_api.rs`
- `sdk/src/client.rs`
- `frontend/src/App.tsx`

These files are not only large; their responsibilities are mixed, which makes later feature work risky.

### 2.3 Contract definitions are duplicated across layers

The backend `sdk_api.rs` defines many SDK request structs locally, while `sdk/src/models.rs` also defines the SDK-side request and response models. This creates a long-term contract drift risk.

### 2.4 Router and control-plane modules are overloaded

- `backend/src/api/mod.rs` is one large route registry
- `backend/src/repositories/control_plane.rs` mixes model profiles, job policies, and job runs in one file

These are good split candidates because their boundaries are already visible.

### 2.5 Thin refactor safety net

No meaningful first-party tests were found in `backend/src`, `frontend/src`, or `sdk/src`. That means large refactors should be done incrementally and with a small test layer added first.

## 3. Highest-Value Abstractions

### 3.1 Extract a shared AI pipeline module

Recommended new module:

```text
backend/src/services/ai_pipeline/
  mod.rs
  chunking.rs
  overflow.rs
  summarizer.rs
  formatting.rs
  types.rs
```

Suggested ownership:

- `chunking.rs`
  - `estimate_tokens_text`
  - `split_text_by_token_limit`
  - `split_chunks_by_token_limit`
- `overflow.rs`
  - `is_context_overflow_error`
  - overflow retry policy
- `summarizer.rs`
  - `summarize_texts_with_split`
  - `summarize_texts_once`
  - `merge_chunk_summaries`
- `formatting.rs`
  - `build_ai_input`
  - common output decoration helpers
- `types.rs`
  - shared result structs like `SummaryBuildResult`

After this extraction:

- `summary.rs` should only keep thread-summary business rules
- `subject_memory.rs` should only keep subject-memory business rules

### 3.2 Extract common job-run lifecycle helpers

Both scheduler jobs and direct-run flows repeatedly do:

- create job run
- execute domain logic
- finish job run
- report counts and error metadata

Recommended helper module:

```text
backend/src/services/job_runtime.rs
```

This helper should wrap the repetitive job bookkeeping, so domain services only focus on selection and generation logic.

### 3.3 Extract shared metadata and digest helpers

The subject-memory flow already has standalone utility-style logic such as:

- digest generation
- metadata normalization
- project-id extraction

These should become utilities instead of staying inside the main service file.

## 4. Large File Split Plan

### 4.1 `backend/src/services/summary.rs`

Current responsibilities:

- thread summary generation
- repair summary generation
- thread rollup generation
- record selection
- oversized record handling
- prompt building
- AI chunking and merge logic
- rollup batch selection

Recommended split:

```text
backend/src/services/summary/
  mod.rs
  settings.rs
  thread_summary.rs
  thread_repair.rs
  rollup.rs
  selectors.rs
  render.rs
```

Recommended boundary:

- `thread_summary.rs`: normal summary generation
- `thread_repair.rs`: repair-specific summary flow
- `rollup.rs`: rollup scheduling and rollup execution
- `selectors.rs`: pending-record and rollup candidate selection
- `settings.rs`: policy loading and normalization
- `render.rs`: record-to-text and summary-to-text formatting

Priority: `P0`

### 4.2 `backend/src/services/subject_memory.rs`

Current responsibilities:

- subject-memory L0 generation
- subject-memory rollup generation
- scope runner
- policy resolution
- summary selection
- rollup selection
- metadata construction
- digest generation
- duplicated AI chunking pipeline

Recommended split:

```text
backend/src/services/subject_memory/
  mod.rs
  settings.rs
  from_summaries.rs
  rollup.rs
  scope_runner.rs
  metadata.rs
  digest.rs
  selectors.rs
```

Priority: `P0`

### 4.3 `backend/src/api/sdk_api.rs`

Current responsibilities:

- SDK auth extractor
- SDK request DTOs
- thread handlers
- record handlers
- summary handlers
- snapshot handlers
- subject-memory handlers
- scheduler trigger handlers

Recommended split:

```text
backend/src/api/sdk/
  mod.rs
  auth.rs
  requests.rs
  threads.rs
  records.rs
  summaries.rs
  snapshots.rs
  subject_memories.rs
  jobs.rs
  context.rs
```

If a shared contracts crate is introduced later, `requests.rs` can disappear and re-export shared types.

Priority: `P1`

### 4.4 `backend/src/api/mod.rs`

Current problem:

- one very long route registration file
- admin, sdk, internal, and public routes mixed together

Recommended split:

```text
backend/src/api/
  mod.rs
  router_admin.rs
  router_sdk.rs
  router_internal.rs
  router_public.rs
```

Or:

```text
backend/src/api/router/
  mod.rs
  admin.rs
  sdk.rs
  internal.rs
  public.rs
```

Then `api/mod.rs` should only merge subrouters.

Priority: `P1`

### 4.5 `backend/src/repositories/control_plane.rs`

Current responsibilities:

- model profile CRUD
- job policy defaults and normalization
- job run lifecycle
- job run stats
- stale job recovery

Recommended split:

```text
backend/src/repositories/control_plane/
  mod.rs
  model_profiles.rs
  job_policies.rs
  job_runs.rs
```

Priority: `P1`

### 4.6 `sdk/src/client.rs`

Current responsibilities:

- auth mode handling
- low-level HTTP transport
- query building
- admin APIs
- thread APIs
- record APIs
- snapshot APIs
- summary APIs
- subject-memory APIs
- job APIs

Recommended split:

```text
sdk/src/client/
  mod.rs
  transport.rs
  auth.rs
  query.rs
  admin.rs
  threads.rs
  records.rs
  snapshots.rs
  summaries.rs
  subject_memory.rs
  jobs.rs
```

Recommended rule:

- `transport.rs` owns request sending and error conversion
- per-domain files own only path assembly and typed methods

Priority: `P0`

### 4.7 `sdk/src/models.rs`

This file is large because it acts as a catch-all contract file.

Recommended split if no new crate is introduced yet:

```text
sdk/src/models/
  mod.rs
  common.rs
  admin.rs
  threads.rs
  records.rs
  snapshots.rs
  summaries.rs
  subject_memory.rs
  jobs.rs
```

Better long-term option:

```text
crates/memory_engine_contracts/
```

and then:

- backend uses shared request/response structs
- sdk re-exports those structs
- frontend can later generate types from the same contract source

Priority: `P1`

### 4.8 `frontend/src/App.tsx`

Current responsibilities:

- app shell
- page switching
- async loading
- filter forms
- modal control
- table definitions
- page rendering
- inline helper functions
- inline editor component

Recommended split:

```text
frontend/src/
  App.tsx
  app/
    AppShell.tsx
    routes.ts
  pages/
    DashboardPage.tsx
    DataPage.tsx
    SourcesPage.tsx
    ModelsPage.tsx
    PoliciesPage.tsx
    RunsPage.tsx
  components/
    PolicyEditorCard.tsx
    SourceModal.tsx
    ModelModal.tsx
    tables/
      sourceColumns.tsx
      modelColumns.tsx
      runColumns.tsx
      threadColumns.tsx
      recordColumns.tsx
      summaryColumns.tsx
      subjectMemoryColumns.tsx
  hooks/
    useDashboardData.ts
    useSources.ts
    useModels.ts
    usePolicies.ts
    useRuns.ts
    useThreads.ts
  utils/
    format.ts
    recordTools.ts
```

Priority: `P0`

### 4.9 `frontend/src/api.ts` and `frontend/src/types.ts`

These are not yet huge, but they are already becoming domain aggregators.

Recommended split:

```text
frontend/src/api/
  client.ts
  admin.ts
  threads.ts
  summaries.ts
  subjectMemories.ts
  jobs.ts

frontend/src/types/
  admin.ts
  threads.ts
  summaries.ts
  subjectMemories.ts
  jobs.ts
  index.ts
```

Priority: `P2`

## 5. Recommended Target Structure

```text
memory_engine/
  backend/src/
    ai/
    api/
      router/
      sdk/
    repositories/
      control_plane/
    services/
      ai_pipeline/
      summary/
      subject_memory/
      job_runtime.rs
  sdk/src/
    client/
    models/
  frontend/src/
    app/
    pages/
    components/
    hooks/
    api/
    types/
    utils/
  crates/
    memory_engine_contracts/
```

## 6. Refactor Order

### Phase 0: Add a small safety net

Before splitting large files, add tests around the code most likely to regress:

- chunk splitting behavior
- overflow retry behavior
- policy normalization
- digest generation
- query-string generation in SDK client

This phase is important because current test coverage appears very thin.

### Phase 1: Extract shared logic without changing APIs

Do first:

1. Extract `services/ai_pipeline`
2. Extract `services/job_runtime.rs`
3. Move utility functions out of `subject_memory.rs`
4. Keep public function signatures unchanged

This gives immediate maintenance benefits with relatively low risk.

### Phase 2: Split the biggest service files

Do next:

1. Split `backend/src/services/summary.rs`
2. Split `backend/src/services/subject_memory.rs`
3. Split `sdk/src/client.rs`
4. Split `frontend/src/App.tsx`

This is the largest readability win.

### Phase 3: Split route and repository aggregators

Do after service-layer cleanup:

1. Split `backend/src/api/sdk_api.rs`
2. Split `backend/src/api/mod.rs`
3. Split `backend/src/repositories/control_plane.rs`

This makes the backend entry surface easier to extend.

### Phase 4: Unify shared contracts

Recommended when the earlier refactors are stable:

1. Introduce `crates/memory_engine_contracts`
2. Move shared SDK request and response structs into it
3. Make backend and sdk both depend on it
4. Consider generating frontend types from the same source later

This is slightly more invasive, so it should not be the first refactor.

## 7. Priority Summary

### P0

- Extract shared AI summarization pipeline
- Split `backend/src/services/summary.rs`
- Split `backend/src/services/subject_memory.rs`
- Split `sdk/src/client.rs`
- Split `frontend/src/App.tsx`
- Add minimal tests before major moves

### P1

- Split `backend/src/api/sdk_api.rs`
- Split `backend/src/api/mod.rs`
- Split `backend/src/repositories/control_plane.rs`
- Split `sdk/src/models.rs`
- Introduce shared job-runtime helpers

### P2

- Split `frontend/src/api.ts` and `frontend/src/types.ts`
- Split `backend/src/ai/mod.rs` further if provider logic keeps growing
- Generate frontend types from shared contracts

## 8. First PR Recommendation

If this work is done incrementally, the safest first PR is:

1. Add unit tests for chunk splitting, overflow classification, and digest generation
2. Introduce `backend/src/services/ai_pipeline`
3. Migrate `summary.rs` to use the shared pipeline
4. Migrate `subject_memory.rs` to use the shared pipeline
5. Keep all HTTP APIs and response shapes unchanged

This first PR gives the best cost-benefit ratio and reduces future refactor duplication immediately.

## 9. What Not To Do First

- Do not split every large file in one PR
- Do not introduce a contracts crate before shared logic extraction is stable
- Do not make backend depend directly on the current SDK crate
  - the SDK crate contains transport concerns and is not a good backend dependency boundary

## 10. Expected Outcome

After the `P0 + P1` work is complete, the project should gain:

- lower duplicate logic in the AI processing path
- smaller and more focused service files
- clearer backend routing boundaries
- easier SDK maintenance
- easier frontend page-level iteration
- lower risk of contract drift across backend and SDK
