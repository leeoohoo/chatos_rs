// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import React from 'react';
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { ApiClientProvider } from '../../lib/api/ApiClientContext';
import type ApiClient from '../../lib/api/client';
import type { Session } from '../../types';
import { useSessionRuntimeSettings } from './useSessionRuntimeSettings';

describe('useSessionRuntimeSettings', () => {
  it('persists an unbound model selection to the session created for the first message', async () => {
    const getConversationRuntimeSettings = vi.fn();
    const updateConversationRuntimeSettings = vi.fn(async (_sessionId, payload) => payload);
    const client = {
      getConversationRuntimeSettings,
      updateConversationRuntimeSettings,
    } as unknown as ApiClient;
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <ApiClientProvider client={client}>{children}</ApiClientProvider>
    );
    const initialProps: { session: Session | null } = { session: null };
    const { result, rerender } = renderHook(
      ({ session }: { session: Session | null }) => useSessionRuntimeSettings({ session }),
      {
        initialProps,
        wrapper,
      },
    );

    act(() => {
      result.current.setModelRuntimeSelection({
        selectedModelId: 'model-1',
        selectedModelName: 'gpt-test',
        selectedThinkingLevel: 'high',
      });
    });

    await act(async () => {
      await result.current.flushRuntimeSettings('session-new');
    });

    expect(updateConversationRuntimeSettings).toHaveBeenCalledWith(
      'session-new',
      expect.objectContaining({
        selected_model_id: 'model-1',
        selected_model_name: 'gpt-test',
        selected_thinking_level: 'high',
      }),
    );
    expect(result.current.selectedModelId).toBe('model-1');

    rerender({ session: { id: 'session-new' } as Session });

    expect(result.current.selectedModelId).toBe('model-1');
    expect(getConversationRuntimeSettings).not.toHaveBeenCalled();
  });

  it('keeps an unbound model draft when the new session renders before the send flush', async () => {
    const getConversationRuntimeSettings = vi.fn();
    const updateConversationRuntimeSettings = vi.fn(async (_sessionId, payload) => payload);
    const client = {
      getConversationRuntimeSettings,
      updateConversationRuntimeSettings,
    } as unknown as ApiClient;
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <ApiClientProvider client={client}>{children}</ApiClientProvider>
    );
    const { result, rerender } = renderHook(
      ({ session }: { session: Session | null }) => useSessionRuntimeSettings({ session }),
      {
        initialProps: { session: null as Session | null },
        wrapper,
      },
    );

    act(() => {
      result.current.setModelRuntimeSelection({
        selectedModelId: 'model-first-message',
        selectedModelName: 'gpt-first-message',
        selectedThinkingLevel: 'medium',
      });
    });

    rerender({ session: { id: 'session-created-before-flush' } as Session });

    expect(result.current.selectedModelId).toBe('model-first-message');
    expect(getConversationRuntimeSettings).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.flushRuntimeSettings('session-created-before-flush');
    });

    expect(updateConversationRuntimeSettings).toHaveBeenCalledWith(
      'session-created-before-flush',
      expect.objectContaining({
        selected_model_id: 'model-first-message',
        selected_model_name: 'gpt-first-message',
        selected_thinking_level: 'medium',
      }),
    );
    expect(result.current.selectedModelId).toBe('model-first-message');
  });
});
