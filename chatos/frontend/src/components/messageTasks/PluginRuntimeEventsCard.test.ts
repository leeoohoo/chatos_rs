// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import { pluginRuntimeSummariesFromEvents } from './pluginRuntimeEvents';

describe('pluginRuntimeSummariesFromEvents', () => {
  it('projects only allowlisted Plugin runtime and Hook metadata', () => {
    const summaries = pluginRuntimeSummariesFromEvents([
      {
        id: 'event-ignored',
        run_id: 'run-1',
        event_type: 'tool_stream',
        payload: { result: 'secret-result' },
      },
      {
        id: 'event-runtime',
        run_id: 'run-1',
        event_type: 'plugin_runtime',
        created_at: '2026-07-26T00:00:00Z',
        payload: {
          plugin_id: 'plugin-browser',
          release_id: 'release-1',
          component_key: 'browser.hooks/v1',
          adapter_session_id: 'session-1',
          phase: 'execute',
          status: 'failed',
          operation: 'dispatch_hook_event',
          duration_ms: 25,
          error: 'approval declined',
          stdout: 'must-not-render',
          hook_dispatch: {
            event: 'PreToolUse',
            blocking_failure: false,
            executions: [
              {
                matched: true,
                succeeded: false,
                timed_out: false,
                workspace_write: true,
                workspace_write_approved: false,
                stdout_sha256: 'a'.repeat(64),
              },
            ],
          },
        },
      },
      {
        id: 'event-blocked',
        run_id: 'run-1',
        event_type: 'plugin_hook_blocked',
        message: 'Plugin Hook PreToolUse failed',
        payload: {
          event: 'PreToolUse',
          tool_name: 'browser_snapshot',
          blocking_failure: true,
          raw_payload: 'must-not-render-either',
        },
      },
    ]);

    expect(summaries).toHaveLength(2);
    expect(summaries[0]).toMatchObject({
      pluginId: 'plugin-browser',
      releaseId: 'release-1',
      componentKey: 'browser.hooks/v1',
      sessionId: 'session-1',
      phase: 'execute',
      status: 'failed',
      operation: 'dispatch_hook_event',
      durationMs: 25,
      error: 'approval declined',
      hook: {
        event: 'PreToolUse',
        executions: 1,
        matched: 1,
        failed: 1,
        timedOut: 0,
        workspaceWriteRequested: 1,
        workspaceWriteApproved: 0,
        workspaceWriteDenied: 1,
      },
    });
    expect(summaries[1]).toMatchObject({
      eventType: 'plugin_hook_blocked',
      phase: 'hook',
      status: 'blocked',
      operation: 'PreToolUse',
      toolName: 'browser_snapshot',
    });
    expect(JSON.stringify(summaries)).not.toContain('must-not-render');
    expect(JSON.stringify(summaries)).not.toContain('stdout_sha256');
  });
});
