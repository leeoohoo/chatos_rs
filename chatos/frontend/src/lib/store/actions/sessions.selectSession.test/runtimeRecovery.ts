// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { describe, expect, it } from 'vitest';

import type { ChatStoreDraft } from '../../types';
import { applyRuntimeSnapshotRecovery } from '../sessions/runtimeRecovery';

const state = (): ChatStoreDraft => ({
  currentSessionId: 'session-1',
  sessions: [{ id: 'session-1' }],
  sessionChatState: {},
  isLoading: false,
  isStreaming: false,
  streamingMessageId: null,
} as unknown as ChatStoreDraft);

describe('session runtime recovery', () => {
  it('restores an active backend turn after page reload', () => {
    const draft = state();

    applyRuntimeSnapshotRecovery(draft, 'session-1', {
      conversation_id: 'session-1',
      turn_id: 'turn-active',
      status: 'running',
      snapshot_source: 'captured',
      active_in_runtime: true,
      snapshot: null,
    });

    expect(draft.sessionChatState['session-1']).toMatchObject({
      isLoading: true,
      activeTurnId: 'turn-active',
      streamingTransport: 'realtime',
    });
    expect(draft.isLoading).toBe(true);
  });

  it('does not revive an orphaned running snapshot', () => {
    const draft = state();

    applyRuntimeSnapshotRecovery(draft, 'session-1', {
      conversation_id: 'session-1',
      turn_id: 'turn-orphaned',
      status: 'running',
      snapshot_source: 'captured',
      active_in_runtime: false,
      snapshot: null,
    });

    expect(draft.sessionChatState['session-1']).toBeUndefined();
    expect(draft.isLoading).toBe(false);
  });

  it('does not let an older snapshot overwrite a newer active turn', () => {
    const draft = state();
    draft.sessionChatState['session-1'] = {
      isLoading: true,
      isStreaming: false,
      isStopping: false,
      streamingPhase: null,
      streamingMessageId: null,
      activeTurnId: 'turn-new',
      streamingPreviewText: '',
      streamingTransport: 'realtime',
      runtimeContextRefreshNonce: 0,
    };

    applyRuntimeSnapshotRecovery(draft, 'session-1', {
      conversation_id: 'session-1',
      turn_id: 'turn-old',
      status: 'running',
      snapshot_source: 'captured',
      active_in_runtime: true,
      snapshot: null,
    });

    expect(draft.sessionChatState['session-1']?.activeTurnId).toBe('turn-new');
  });
});
