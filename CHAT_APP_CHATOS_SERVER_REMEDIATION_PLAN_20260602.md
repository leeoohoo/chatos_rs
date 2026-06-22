# chat_app / chat_app_server_rs Remediation Plan

## Scope

This plan covers:

- `chat_app`
- `chat_app_server_rs`

It combines:

1. the design issues found in the static review
2. the two new UX fixes you requested:
   - runtime guidance cannot paste images today
   - pasted images sometimes become too large and fail at request time

This document is a delivery plan first. No behavior changes are included here yet.

## Confirmed Problems

### P0. WebSocket auth token can leak into logs

Current behavior:

- frontend appends `access_token` into websocket query string
- backend accepts websocket auth from query string
- backend tracing logs the full request URI

Evidence:

- `chat_app/src/lib/realtime/buildWsUrl.ts`
- `chat_app_server_rs/src/api/mod.rs`

Impact:

- token exposure in logs, reverse proxies, browser tooling, and diagnostics

### P1. Secret handling is inconsistent across modules

Current behavior:

- `remote_connections` already encrypts password fields before storage
- `ai_model_configs` stores `api_key` in plaintext and returns it in normal API responses

Evidence:

- `chat_app_server_rs/src/repositories/remote_connections.rs`
- `chat_app_server_rs/src/repositories/ai_model_configs.rs`
- `chat_app_server_rs/src/api/configs/ai_model.rs`

Impact:

- secret storage policy is not unified
- future secret-bearing modules will likely drift again

### P1. Frontend persisted state is not isolated by user

Current behavior:

- auth state and chat state are persisted separately
- chat store persistence key is global
- logout clears auth state but does not clear chat store persistence

Evidence:

- `chat_app/src/lib/auth/authStore.ts`
- `chat_app/src/lib/store/createChatStoreWithBackend.ts`

Impact:

- model / agent / session-runtime preferences can bleed across users on the same browser profile

### P2. Frontend client/store injection is only partially implemented

Current behavior:

- app supports context-provided store and custom `ApiClient`
- many modules still import the global singleton directly

Evidence:

- `chat_app/src/lib/store/ChatStoreContext.tsx`
- `chat_app/src/lib/realtime/RealtimeProvider.tsx`
- `chat_app/src/i18n/I18nProvider.tsx`
- `chat_app/src/components/AgentManager.tsx`

Impact:

- multi-instance embedding is fragile
- test isolation is weaker than it looks
- future multi-backend or per-pane runtime isolation will be harder

### P2. Health semantics do not match startup dependency reality

Current behavior:

- startup performs several side-effectful bootstraps
- multiple failures are downgraded to warnings
- `/health` still reports green

Evidence:

- `chat_app_server_rs/src/modules/app_startup.rs`
- `chat_app_server_rs/src/api/mod.rs`

Impact:

- service can look healthy while key capabilities are degraded

### P3. `chat_app_server_rs` is not self-contained to build

Current behavior:

- Rust crate depends on an absolute-path local SDK

Evidence:

- `chat_app_server_rs/Cargo.toml`

Impact:

- CI portability and teammate onboarding are brittle

### P1. Runtime guidance path does not support image input

Current behavior:

- guide mode disables attachments in the composer
- entering guiding mode clears existing attachments
- `GuideMessageHandler` only accepts text
- `/api/agent/chat/guide` only accepts `content`
- runtime guidance is stored and replayed as plain text only

Evidence:

- `chat_app/src/components/inputArea/useInputAreaController.ts`
- `chat_app/src/components/inputArea/useInputAreaMessageDraft.ts`
- `chat_app/src/types/runtime.ts`
- `chat_app/src/lib/api/client/stream.ts`
- `chat_app_server_rs/src/api/agent_chat/runtime_guidance.rs`
- `chat_app_server_rs/src/modules/conversation_runtime/guidance.rs`
- `chat_app_server_rs/src/services/agent_runtime/ai_client/execution_loop_guidance.rs`

Impact:

- user cannot add visual guidance while a turn is running
- the most natural debugging flow for UI / screenshot issues is blocked

### P1. Image attachment size policy is internally inconsistent

Current behavior:

- frontend accepts up to `20MB` per file and `50MB` total
- image attachments are converted to data URL without send-time compression
- backend prechecks upstream request size with default limit `1_500_000` bytes
- base64 expansion makes the real payload much larger than the original image file

Evidence:

- `chat_app/src/components/inputArea/fileUtils.ts`
- `chat_app/src/lib/store/actions/sendMessage/attachments.ts`
- `chat_app_server_rs/src/services/agent_runtime/ai_request_handler/mod.rs`
- `chat_app_server_rs/src/services/ai_common/request_support/request_transport.rs`

Impact:

- UX says image is accepted, but request later fails
- failure happens late and feels random

## Root Causes

### Root Cause A. Capability boundaries are represented in UI state, not in shared contracts

Symptoms:

- guide mode image support was blocked in the composer instead of modeled in a shared runtime contract
- secret handling differs by module
- health is not tied to actual runtime capability readiness

### Root Cause B. Attachment pipeline has no payload-budget model

Symptoms:

- file-size validation happens on raw file size
- request-size validation happens much later on expanded JSON payload
- no shared notion of "budget for upstream model request"

### Root Cause C. Singleton-first frontend design conflicts with per-user and per-context runtime

Symptoms:

- persisted chat state is global
- `ApiClient` is injectable in theory but global in practice

## Delivery Strategy

## Phase 1. Safety and unblockers

Goal:

- remove the most dangerous leakage
- unblock image guidance
- stop oversized-image late failures

Planned changes:

### 1. Redact websocket auth from request logging

Server changes:

- stop logging raw request URI in the auth middleware trace span
- log path only, or a redacted URI without sensitive query params
- explicitly redact `access_token`, `token`, and other auth-like query keys

Preferred direction:

- immediate fix: redact in logs
- follow-up hardening: replace query-token auth with short-lived websocket ticket or one-shot session key

### 2. Add structured runtime guidance attachments end-to-end

Frontend changes:

- change `GuideMessageHandler` from text-only to `(content, attachments, options?)`
- keep attachment support enabled during guide mode
- remove the effect that clears attachments when guide mode starts
- reuse the existing attachment picker / paste / drop flow in guide mode

Frontend API changes:

- extend `submitRuntimeGuidance` payload to include `attachments`

Backend API changes:

- extend `/api/agent/chat/guide` request schema to accept attachment payloads
- parse them using the existing attachment model where possible

Backend runtime changes:

- evolve runtime guidance item from plain text into structured guidance input
- store text plus attachments metadata
- when draining runtime guidance, build message parts rather than a text-only system string

Recommended injection model:

- preserve the current "high-priority runtime guidance" semantics
- encode it as a structured message item with:
  - a leading high-priority runtime guidance text marker
  - optional image parts
- reuse existing provider adaptation logic for non-vision models

Why not patch only the frontend:

- the current backend queue stores only text
- even if paste becomes possible in UI, the image would be dropped before reaching the model

### 3. Add a real attachment payload budget and client-side image compression

Frontend changes:

- add an estimated request payload budget for inline attachments
- before send, compress image attachments for transport
- keep original file for local preview, but send compressed transport blob

Recommended transport policy:

- resize long edge to a bounded maximum
- re-encode iteratively until target byte budget is met
- prefer:
  - WebP or JPEG for photos / screenshots without alpha constraints
  - PNG or WebP only when transparency matters

Recommended first-pass targets:

- per inline image transport target: around `700KB` to `900KB`
- total inline attachment transport budget: keep well below backend default request limit

Late-failure prevention:

- estimate final JSON payload size before send
- if still too large after compression:
  - show a user-facing error before request
  - explain actual limit and next action

Backend changes:

- keep the server-side payload precheck
- return a typed, user-facing error code for oversized attachment payload
- include actual payload bytes and effective limit in the error payload

### 4. Align frontend limits with backend reality

Frontend changes:

- split "pickable file size" from "inline model payload size"
- keep generous raw-file limits for future file-upload architecture if needed
- but enforce a much smaller inline-image transport budget for current chat path

Recommended UX rule:

- images for chat/guidance are "inline model inputs", not generic uploads
- therefore they need a separate visible limit policy

## Phase 2. Consistency fixes

Goal:

- make current behavior coherent across modules

### 5. Unify secret storage policy

Planned direction:

- create one shared secret-storage rule for all stored credentials
- apply it to:
  - `remote_connections`
  - `ai_model_configs`
  - future model / gateway / remote secrets

Recommended changes:

- encrypt `ai_model_configs.api_key` at rest
- stop returning raw `api_key` in list responses by default
- support explicit "reveal secret" only where strictly necessary and authorized

### 6. Isolate persisted frontend state per user

Planned direction:

- scope persisted chat-store key by user id
- clear or migrate persisted chat state on logout / account switch

Recommended behavior:

- auth store remains global
- user-bound runtime preferences become user-scoped

### 7. Finish dependency injection cleanup in `chat_app`

Planned direction:

- remove direct imports of global `apiClient` from feature modules that already live inside provider context
- route them through context or through injected helpers

Priority targets:

- realtime provider
- i18n provider
- manager panels that currently bypass context

## Phase 3. Operability and maintainability

Goal:

- reduce future drift

### 8. Improve health reporting semantics

Planned direction:

- keep `/health` for basic liveness
- add a richer readiness / capability status endpoint

Recommended fields:

- db ready
- auth bootstrap ready
- memory engine source ready
- watcher state
- degraded capability list

### 9. Remove absolute-path build dependency

Planned direction:

- convert local absolute-path SDK dependency into:
  - workspace dependency, or
  - vendored submodule / sibling path configurable by workspace, or
  - optional feature with explicit setup

## Implementation Order I Recommend

Order for the next coding pass:

1. websocket log redaction
2. runtime guidance attachment contract end-to-end
3. client-side image compression and request-budget precheck
4. backend typed oversize errors
5. persisted-state user isolation
6. `ai_model_configs` secret handling alignment
7. singleton client cleanup
8. health/readiness refinement
9. build portability cleanup

## Files Expected To Change

### `chat_app`

- `src/components/inputArea/useInputAreaController.ts`
- `src/components/inputArea/useInputAreaMessageDraft.ts`
- `src/components/inputArea/useAttachmentsInput.ts`
- `src/components/inputArea/fileUtils.ts`
- `src/lib/store/actions/sendMessage/attachments.ts`
- `src/lib/api/client/stream.ts`
- `src/lib/api/client/types/*`
- `src/types/runtime.ts`
- `src/lib/auth/authStore.ts`
- `src/lib/store/createChatStoreWithBackend.ts`
- `src/lib/realtime/RealtimeProvider.tsx`
- plus modules that directly import the global `apiClient`

### `chat_app_server_rs`

- `src/api/mod.rs`
- `src/api/agent_chat/runtime_guidance.rs`
- `src/modules/conversation_runtime/guidance.rs`
- `src/services/runtime_guidance_manager/*`
- `src/services/agent_runtime/ai_client/execution_loop_guidance.rs`
- `src/services/ai_common/request_support/request_transport.rs`
- `src/repositories/ai_model_configs.rs`
- `src/api/configs/ai_model.rs`
- `src/modules/app_startup.rs`
- `Cargo.toml`

## Acceptance Criteria

### For runtime guidance image support

- while a turn is running, user can paste or pick an image in guide mode
- guide submission can include text only, image only, or both
- image guidance reaches the model path instead of being dropped in UI or queue
- non-vision models degrade predictably and visibly

### For oversized image prevention

- user gets a local validation / compression result before request send
- ordinary screenshots no longer fail due to request size in normal use
- when a request still cannot fit, the error is explicit and actionable

### For design fixes

- websocket logs no longer expose auth tokens
- `ai_model_configs` no longer stores or echoes secrets in plaintext flows
- chat persistence is isolated by user
- key feature modules stop bypassing injected runtime context

## Risks And Notes

### Runtime guidance image support is not a tiny patch

Reason:

- current guidance queue is text-only by design
- the correct fix is contract-level, not just UI-level

### Image compression must preserve enough fidelity for screenshots

Reason:

- aggressive compression can make UI screenshots unreadable
- transport compression should be adaptive, not one fixed quality value

### There is a short-term and long-term fix for websocket auth

Short-term:

- redact logs immediately

Long-term:

- move away from query-string bearer token transport

## What I Will Do After You Confirm

I recommend implementing in this order:

1. P0 websocket token redaction
2. runtime guidance image support end-to-end
3. image compression + payload-budget guard
4. user-scoped chat persistence
5. `ai_model_configs` secret alignment

That sequence gives the highest user-visible value first while also removing the most dangerous risk.
