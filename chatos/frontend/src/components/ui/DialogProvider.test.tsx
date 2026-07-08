// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import React from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { DialogProvider, useDialogService } from './DialogProvider';

const DialogHarness: React.FC = () => {
  const dialogs = useDialogService();

  return (
    <div>
      <button
        type="button"
        onClick={() => {
          void dialogs.alert({
            title: '模型缺失',
            message: '请先选择一个模型',
            type: 'warning',
          });
        }}
      >
        open alert
      </button>
      <button
        type="button"
        onClick={() => {
          void dialogs.confirm({
            title: '删除会话',
            message: '确认删除当前会话？',
          });
        }}
      >
        open confirm
      </button>
      <button
        type="button"
        onClick={() => {
          void dialogs.prompt({
            title: '重命名',
            message: '请输入新名称',
            defaultValue: '旧名称',
            validate: (value) => (value.trim() ? null : '名称不能为空'),
          });
        }}
      >
        open prompt
      </button>
    </div>
  );
};

const renderProvider = () => render(
  <DialogProvider>
    <DialogHarness />
  </DialogProvider>,
);

describe('DialogProvider', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders alert without using native browser dialogs', async () => {
    const nativeAlert = vi.spyOn(window, 'alert').mockImplementation(() => undefined);

    renderProvider();
    fireEvent.click(screen.getByRole('button', { name: 'open alert' }));

    expect(screen.getByText('请先选择一个模型')).toBeInTheDocument();
    expect(nativeAlert).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: '知道了' }));
    await waitFor(() => {
      expect(screen.queryByText('请先选择一个模型')).not.toBeInTheDocument();
    });
  });

  it('renders confirm without using native browser dialogs', () => {
    const nativeConfirm = vi.spyOn(window, 'confirm').mockReturnValue(true);

    renderProvider();
    fireEvent.click(screen.getByRole('button', { name: 'open confirm' }));

    expect(screen.getByText('确认删除当前会话？')).toBeInTheDocument();
    expect(nativeConfirm).not.toHaveBeenCalled();
  });

  it('renders prompt without using native browser dialogs', () => {
    const nativePrompt = vi.spyOn(window, 'prompt').mockReturnValue('新名称');

    renderProvider();
    fireEvent.click(screen.getByRole('button', { name: 'open prompt' }));

    expect(screen.getByDisplayValue('旧名称')).toBeInTheDocument();
    expect(nativePrompt).not.toHaveBeenCalled();
  });

  it('settles the active dialog when a different dialog opens', async () => {
    renderProvider();
    fireEvent.click(screen.getByRole('button', { name: 'open confirm' }));
    expect(screen.getByText('确认删除当前会话？')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'open prompt' }));

    await waitFor(() => {
      expect(screen.queryByText('确认删除当前会话？')).not.toBeInTheDocument();
    });
    expect(screen.getByDisplayValue('旧名称')).toBeInTheDocument();
  });

  it('requires DialogProvider for service consumers', () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);

    expect(() => render(<DialogHarness />)).toThrow(
      'useDialogService must be used within DialogProvider',
    );

    consoleError.mockRestore();
  });
});
