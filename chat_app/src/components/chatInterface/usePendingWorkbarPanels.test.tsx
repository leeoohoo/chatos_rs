// @vitest-environment jsdom

import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { usePendingWorkbarPanels } from './usePendingWorkbarPanels';
import type { SessionWorkbarApiClient } from './useSessionWorkbarPanels.types';

const realtimeState = {
  value: 'connected',
};

vi.mock('../../lib/realtime/RealtimeProvider', () => ({
  useRealtimeConnectionState: () => realtimeState.value,
}));

describe('usePendingWorkbarPanels', () => {
  afterEach(() => {
    realtimeState.value = 'connected';
    vi.clearAllMocks();
  });

  it('loads pending task review panels on first sync even when realtime is connected', async () => {
    const getPendingTaskReviews = vi.fn(async () => ([
      {
        review_id: 'review-1',
        conversation_id: 'session-1',
        conversation_turn_id: 'turn-1',
        draft_tasks: [],
      },
    ]));
    const getPendingUiPrompts = vi.fn(async () => []);
    const upsertTaskReviewPanel = vi.fn();
    const removeTaskReviewPanel = vi.fn();
    const upsertUiPromptPanel = vi.fn();
    const removeUiPromptPanel = vi.fn();

    renderHook(() => usePendingWorkbarPanels({
      apiClient: {
        getPendingTaskReviews,
        getPendingUiPrompts,
        getTaskManagerTasks: vi.fn(async () => []),
        completeTaskManagerTask: vi.fn(async () => ({ id: 'task-1' })),
        deleteTaskManagerTask: vi.fn(async () => ({ success: true })),
        updateTaskManagerTask: vi.fn(async () => ({ id: 'task-1' })),
        submitTaskReviewDecision: vi.fn(async () => ({})),
        submitUiPromptResponse: vi.fn(async () => ({})),
      } satisfies SessionWorkbarApiClient,
      enabled: true,
      sessionId: 'session-1',
      taskReviewPanelsBySession: {},
      uiPromptPanelsBySession: {},
      upsertTaskReviewPanel,
      removeTaskReviewPanel,
      upsertUiPromptPanel,
      removeUiPromptPanel,
    }));

    await waitFor(() => {
      expect(getPendingTaskReviews).toHaveBeenCalledWith('session-1', { limit: 50 });
    });
    expect(upsertTaskReviewPanel).toHaveBeenCalledWith(expect.objectContaining({
      reviewId: 'review-1',
      sessionId: 'session-1',
      conversationTurnId: 'turn-1',
    }));
    expect(getPendingUiPrompts).toHaveBeenCalledWith('session-1', { limit: 50 });
  });
});
