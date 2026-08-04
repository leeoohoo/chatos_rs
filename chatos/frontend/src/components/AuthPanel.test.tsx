// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

const authStore = vi.hoisted(() => ({
  login: vi.fn(),
  register: vi.fn(),
  sendRegisterEmailCode: vi.fn(),
  clearError: vi.fn(),
}));

vi.mock('@/lib/auth/authStore', () => ({
  useAuthStoreFromContext: () => ({
    ...authStore,
    loading: false,
    error: null,
  }),
}));

vi.mock('../i18n/I18nProvider', () => ({
  useI18n: () => ({
    t: (key: string) => ({
      'auth.login': '登录',
      'auth.loginHelper': '使用平台账号继续',
      'auth.switchToRegister': '没有账号？去注册',
      'auth.username': '用户名',
      'auth.usernamePlaceholder': '请输入用户名',
      'auth.password': '密码',
      'auth.passwordPlaceholder': '请输入密码',
    })[key] ?? key,
  }),
}));

import { AuthPanel } from './AuthPanel';

describe('AuthPanel account inputs', () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    document.documentElement.classList.remove('dark');
    delete window.chatosLocalRuntime;
  });

  it('keeps account text visible when the application is using the dark theme', () => {
    document.documentElement.classList.add('dark');

    render(<AuthPanel />);

    const usernameInput = screen.getByPlaceholderText('请输入用户名');
    const passwordInput = screen.getByPlaceholderText('请输入密码');

    expect(usernameInput).toHaveAttribute('type', 'text');
    expect(usernameInput).toHaveClass(
      'bg-white',
      'text-gray-900',
      'placeholder:text-gray-400',
      'caret-gray-900',
    );
    expect(usernameInput).toHaveAttribute('autocapitalize', 'none');
    expect(usernameInput).toHaveAttribute('autocorrect', 'off');
    expect(usernameInput).toHaveAttribute('spellcheck', 'false');
    expect(passwordInput).toHaveClass('bg-white', 'text-gray-900', 'caret-gray-900');

  });

  it('restores the local client settings entry before login', () => {
    const openSettings = vi.fn().mockResolvedValue(true);
    window.chatosLocalRuntime = {
      apiRequest: vi.fn(),
      openSettings,
    } as never;

    render(<AuthPanel />);

    fireEvent.click(screen.getByRole('button', { name: '客户端设置' }));

    expect(openSettings).toHaveBeenCalledOnce();
  });
});
