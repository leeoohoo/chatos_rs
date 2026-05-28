import { describe, expect, it } from 'vitest';

import {
  shouldAutoRecoverPendingPanelSession,
} from './pendingPanelRecovery';

describe('pendingPanelRecovery', () => {
  it('recovers when the session is active, idle locally, and the pending panel keeps the turn id', () => {
    expect(shouldAutoRecoverPendingPanelSession({
      targetSessionId: 'session-1',
      currentSessionId: 'session-1',
      activeTurnId: 'turn-1',
      conversationTurnId: 'turn-1',
      isLoading: false,
      isStreaming: false,
      isStopping: false,
    })).toBe(true);
  });

  it('does not recover when the session is already streaming', () => {
    expect(shouldAutoRecoverPendingPanelSession({
      targetSessionId: 'session-1',
      currentSessionId: 'session-1',
      activeTurnId: 'turn-1',
      conversationTurnId: 'turn-1',
      isLoading: false,
      isStreaming: true,
      isStopping: false,
    })).toBe(false);
  });

  it('does not recover when the panel belongs to a different session', () => {
    expect(shouldAutoRecoverPendingPanelSession({
      targetSessionId: 'session-2',
      currentSessionId: 'session-1',
      activeTurnId: 'turn-1',
      conversationTurnId: 'turn-1',
      isLoading: false,
      isStreaming: false,
      isStopping: false,
    })).toBe(false);
  });

  it('does not require an exact turn id match when local active turn is missing', () => {
    expect(shouldAutoRecoverPendingPanelSession({
      targetSessionId: 'session-1',
      currentSessionId: 'session-1',
      activeTurnId: null,
      conversationTurnId: 'turn-1',
      isLoading: false,
      isStreaming: false,
      isStopping: false,
    })).toBe(true);
  });
});
