// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import type { AskUserPromptRecord } from '../../lib/api/client/types';
import ConversationAskUserPromptPanel from './ConversationAskUserPromptPanel';

vi.mock('../../lib/realtime/useConversationAskUserPromptRealtime', () => ({
  useConversationAskUserPromptRealtime: vi.fn(),
}));

const buildChoicePrompt = (): AskUserPromptRecord => ({
  id: 'prompt-1',
  conversation_id: 'lc_session_ask_user',
  conversation_turn_id: 'turn-1',
  kind: 'choice',
  status: 'pending',
  prompt: {
    title: '确认创建本地任务',
    message: 'Task Manager 准备创建 3 个本地任务，是否继续？',
    allow_cancel: true,
    payload: {
      choice: {
        multiple: false,
        min_selections: 1,
        max_selections: 1,
        options: [
          { value: 'confirm', label: '创建任务' },
          { value: 'cancel', label: '取消' },
        ],
      },
    },
  },
});

const renderPanel = (client: Record<string, unknown>) => render(
  <ApiClientProvider client={client as never}>
    <I18nProvider>
      <ConversationAskUserPromptPanel sessionId="lc_session_ask_user" />
    </I18nProvider>
  </ApiClientProvider>,
);

describe('ConversationAskUserPromptPanel', () => {
  afterEach(() => {
    cleanup();
  });

  it('keeps a local choice selected when polling refreshes the same prompt', async () => {
    const listAskUserPrompts = vi
      .fn()
      .mockResolvedValueOnce({ prompts: [buildChoicePrompt()] })
      .mockResolvedValueOnce({ prompts: [buildChoicePrompt()] });

    renderPanel({
      listAskUserPrompts,
      submitAskUserPrompt: vi.fn(),
      cancelAskUserPrompt: vi.fn(),
    });

    const confirmChoice = await screen.findByLabelText('创建任务');
    fireEvent.click(confirmChoice);
    expect(confirmChoice).toBeChecked();

    await act(async () => {
      await new Promise((resolve) => {
        window.setTimeout(resolve, 1100);
      });
    });

    await waitFor(() => expect(listAskUserPrompts).toHaveBeenCalledTimes(2));
    expect(screen.getByLabelText('创建任务')).toBeChecked();
  }, 10_000);

  it('refreshes stale cancelled task runner prompts without showing the raw backend error', async () => {
    const listAskUserPrompts = vi
      .fn()
      .mockResolvedValueOnce({ prompts: [buildChoicePrompt()] })
      .mockResolvedValue({ prompts: [] });
    const submitAskUserPrompt = vi
      .fn()
      .mockRejectedValue(new Error(
        'Task Runner request failed: 400 Bad Request {"error":"提示当前状态不允许提交: cancelled"}',
      ));

    renderPanel({
      listAskUserPrompts,
      submitAskUserPrompt,
      cancelAskUserPrompt: vi.fn(),
    });

    fireEvent.click(await screen.findByLabelText('创建任务'));
    fireEvent.click(screen.getByRole('button', { name: '确认提交' }));

    await waitFor(() => expect(listAskUserPrompts).toHaveBeenCalledTimes(2));
    expect(screen.queryByText(/Task Runner request failed/)).not.toBeInTheDocument();
    expect(screen.queryByText('确认创建本地任务')).not.toBeInTheDocument();
  });
});
