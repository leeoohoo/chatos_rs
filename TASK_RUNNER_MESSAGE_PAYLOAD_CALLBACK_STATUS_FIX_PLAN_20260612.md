# Task Runner Message Payload Callback Status Fix Plan 2026-06-12

## Problems

1. Chatos message "任务" button can request `/api/messages/:id/task-runner/tasks` with a frontend temporary `temp_user_*` id and receive a server error.
2. Task Runner runs can fail with `413 Payload Too Large`, likely after large tool outputs are carried into the next model request.
3. Failed, cancelled, or blocked Task Runner runs are not callbacked to Chatos, so Chatos cannot show the terminal failure.
4. Chatos user-message async status can stay `processing` even after the planner has produced the final summary in Memory Engine.
5. Chatos compact-history can miss later messages even when Memory Engine has them, because full-history pagination stops after a page is filtered for hidden messages.

## Fix Plan

1. Make the Chatos message-task API resilient to temporary message ids by accepting session and turn lookup hints, resolving the persisted user message by turn when the path id is not found, and returning controlled 4xx/502 responses instead of leaking internal errors.
2. Update the frontend task drawer request to pass session/turn/source hints, so clicking "任务" before optimistic messages are replaced still resolves the correct Task Runner source.
3. Add model-request-safe tool-output handling in the shared AI runtime path: large tool outputs should be omitted from model input with a short advisory telling the model to read by line/range, and total tool-output budget should be capped before the next model request.
4. Apply the same tool-output protection when Memory Engine composed records are converted back into model input, because Task Runner iterative runs rebuild context from Memory rather than only appending in-memory tool results.
5. Send Chatos callbacks for all terminal Task Runner statuses: completed, failed, cancelled, and blocked.
6. Ensure final/error realtime payloads include persisted user-message updates so the frontend can replace temporary messages and show `completed` instead of stale `processing`.
7. Fix compact-history full-message pagination to advance and stop based on raw Memory Engine page length, while filtering hidden messages only when accumulating the display list.

## Verification

1. Add focused Rust unit tests for message task lookup fallback, callback event mapping, and tool-output omission.
2. Add focused frontend unit coverage or type-safe implementation for message task lookup hints.
3. Run targeted `cargo test` and frontend tests/type checks where feasible.

## Result

1. Implemented temporary-message fallback lookup for Chatos message task APIs using session/turn/source hints.
2. Implemented frontend task drawer lookup hints and realtime persisted-message fallback parsing.
3. Implemented model-input-safe tool result omission for direct append and Memory Engine composed tool records.
4. Implemented Task Runner callbacks for failed, cancelled, and blocked terminal statuses.
5. Verified with targeted Rust tests, `cargo check` for Chatos and Task Runner backends, `npm run type-check`, and `git diff --check`.
6. Fixed compact-history full-message pagination so hidden-message filtering no longer makes Chatos stop before later Memory Engine records; verified with focused Rust tests and `cargo check -p chat_app_server_rs`.
