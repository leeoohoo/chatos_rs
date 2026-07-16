// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import { DialogProvider } from '../ui/DialogProvider';
import { MemoryTimelineList } from './MemoryTimelineList';

afterEach(cleanup);

const recall = {
  id: 'recall:lc_recall_1',
  sourceId: 'lc_recall_1',
  kind: 'agent_recall' as const,
  text: 'Local project decision',
  time: '2026-07-15T00:00:00Z',
  sourceLabel: '本地项目记忆 L0',
};

const renderList = (
  sessionId: string,
  client: Record<string, unknown>,
  onRefresh = vi.fn(),
) => ({
  onRefresh,
  ...render(
    <ApiClientProvider client={client as never}>
      <I18nProvider>
        <DialogProvider>
          <MemoryTimelineList
            sessionId={sessionId}
            items={[recall]}
            onRefresh={onRefresh}
          />
        </DialogProvider>
      </I18nProvider>
    </ApiClientProvider>,
  ),
});

describe('MemoryTimelineList', () => {
  it('forgets a local recall after confirmation', async () => {
    const deleteConversationMemoryRecall = vi.fn().mockResolvedValue({ success: true });
    const { onRefresh } = renderList('lc_session_memory', {
      deleteConversationMemoryRecall,
    });

    fireEvent.click(screen.getByRole('button', { name: '忘记' }));
    expect(await screen.findByText('忘记本地 Recall')).toBeInTheDocument();
    const forgetButtons = screen.getAllByRole('button', { name: '忘记' });
    fireEvent.click(forgetButtons[forgetButtons.length - 1]);

    await waitFor(() => {
      expect(deleteConversationMemoryRecall).toHaveBeenCalledWith(
        'lc_session_memory',
        'lc_recall_1',
      );
      expect(onRefresh).toHaveBeenCalled();
    });
  });

  it('does not expose local forgetting for cloud sessions', () => {
    renderList('cloud_session_memory', {});
    expect(screen.queryByRole('button', { name: '忘记' })).not.toBeInTheDocument();
  });
});
