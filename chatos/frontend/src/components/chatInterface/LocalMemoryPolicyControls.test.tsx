// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team
// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../i18n/I18nProvider';
import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import { LocalMemoryPolicyControls } from './LocalMemoryPolicyControls';

afterEach(cleanup);

const renderControls = (sessionId: string, client: Record<string, unknown>) => render(
  <ApiClientProvider client={client as never}>
    <I18nProvider>
      <LocalMemoryPolicyControls sessionId={sessionId} />
    </I18nProvider>
  </ApiClientProvider>,
);

describe('LocalMemoryPolicyControls', () => {
  it('loads and saves local session memory policy', async () => {
    const getConversationRuntimeSettings = vi.fn().mockResolvedValue({
      memory_auto_summary_enabled: true,
      memory_summary_message_threshold: 30,
      memory_summary_character_threshold: 48_000,
      memory_recall_limit: 10,
    });
    const updateConversationRuntimeSettings = vi.fn().mockResolvedValue({
      memory_auto_summary_enabled: false,
      memory_summary_message_threshold: 30,
      memory_summary_character_threshold: 48_000,
      memory_recall_limit: 10,
    });
    renderControls('lc_session_policy', {
      getConversationRuntimeSettings,
      updateConversationRuntimeSettings,
    });

    const toggle = await screen.findByLabelText('启用本地自动摘要');
    expect(toggle).toBeChecked();
    expect(screen.getByLabelText('消息记录阈值')).toHaveValue(30);
    expect(screen.getByLabelText('字符数阈值')).toHaveValue(48_000);
    expect(screen.getByLabelText('Recall 上限')).toHaveValue(10);

    fireEvent.click(toggle);
    fireEvent.click(screen.getByRole('button', { name: '保存' }));
    await waitFor(() => {
      expect(updateConversationRuntimeSettings).toHaveBeenCalledWith('lc_session_policy', {
        memory_auto_summary_enabled: false,
        memory_summary_message_threshold: 30,
        memory_summary_character_threshold: 48_000,
        memory_recall_limit: 10,
      });
    });
    expect(await screen.findByText('设置已保存')).toBeInTheDocument();
  });

  it('does not render or load policy for cloud sessions', () => {
    const getConversationRuntimeSettings = vi.fn();
    renderControls('cloud_session_policy', { getConversationRuntimeSettings });

    expect(screen.queryByText('本地自动摘要')).not.toBeInTheDocument();
    expect(getConversationRuntimeSettings).not.toHaveBeenCalled();
  });
});
