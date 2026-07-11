// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';
import { useAuthStoreFromContext } from '@/lib/auth/authStore';
import { useI18n } from '../i18n/I18nProvider';

type AuthMode = 'login' | 'register';

export function AuthPanel() {
  const { login, register, sendRegisterEmailCode, loading, error, clearError } = useAuthStoreFromContext();
  const { t } = useI18n();
  const [mode, setMode] = React.useState<AuthMode>('login');
  const [username, setUsername] = React.useState('');
  const [password, setPassword] = React.useState('');
  const [confirmPassword, setConfirmPassword] = React.useState('');
  const [inviteCode, setInviteCode] = React.useState('');
  const [verificationCode, setVerificationCode] = React.useState('');
  const [codeSending, setCodeSending] = React.useState(false);
  const [localError, setLocalError] = React.useState<string | null>(null);

  const isRegister = mode === 'register';
  const submitLabel = isRegister ? t('auth.registerAndEnter') : t('auth.login');
  const switchLabel = isRegister ? t('auth.switchToLogin') : t('auth.switchToRegister');
  const helperText = isRegister ? t('auth.registerHelper') : t('auth.loginHelper');

  const resetErrors = React.useCallback(() => {
    setLocalError(null);
    clearError();
  }, [clearError]);

  const switchMode = React.useCallback((nextMode: AuthMode) => {
    setMode(nextMode);
    setConfirmPassword('');
    setInviteCode('');
    setVerificationCode('');
    resetErrors();
  }, [resetErrors]);

  const onSendCode = async () => {
    resetErrors();
    const email = username.trim();
    const invite = inviteCode.trim();
    if (!email) {
      setLocalError('请输入邮箱');
      return;
    }
    if (!invite) {
      setLocalError('请输入邀请码');
      return;
    }
    setCodeSending(true);
    try {
      await sendRegisterEmailCode(email, invite);
    } catch {
      // auth store owns the server error
    } finally {
      setCodeSending(false);
    }
  };

  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    resetErrors();

    const trimmedUsername = username.trim();
    if (!trimmedUsername) {
      setLocalError(t('auth.usernameRequired'));
      return;
    }
    if (!password.trim()) {
      setLocalError(t('auth.passwordRequired'));
      return;
    }

    if (isRegister) {
      if (trimmedUsername.length < 3) {
        setLocalError('请输入有效邮箱');
        return;
      }
      if (!inviteCode.trim()) {
        setLocalError('请输入邀请码');
        return;
      }
      if (!verificationCode.trim()) {
        setLocalError('请输入邮箱验证码');
        return;
      }
      if (password.length < 6) {
        setLocalError(t('auth.passwordMinLength'));
        return;
      }
      if (password !== confirmPassword) {
        setLocalError(t('auth.passwordMismatch'));
        return;
      }
    }

    try {
      if (isRegister) {
        await register(trimmedUsername, password, inviteCode.trim(), verificationCode.trim());
      } else {
        await login(trimmedUsername, password);
      }
    } catch {
      // 统一由 auth store 维护服务端错误提示
    }
  };

  const displayError = localError || error;

  return (
    <div className="min-h-screen bg-gray-100 flex items-center justify-center p-4">
      <div className="w-full max-w-md bg-white rounded-lg shadow border border-gray-200 p-6">
        <div className="flex items-center justify-between gap-4 mb-5">
          <div>
            <h1 className="text-xl font-semibold text-gray-900 mb-1">
              {isRegister ? t('auth.register') : t('auth.login')}
            </h1>
            <p className="text-sm text-gray-500">{helperText}</p>
          </div>
          <button
            type="button"
            className="text-sm text-blue-600 hover:text-blue-700"
            onClick={() => switchMode(isRegister ? 'login' : 'register')}
            disabled={loading}
          >
            {switchLabel}
          </button>
        </div>

        <form onSubmit={onSubmit} className="space-y-3">
          <div>
            <label className="block text-sm text-gray-700 mb-1">
              {isRegister ? '邮箱' : t('auth.username')}
            </label>
            <input
              type={isRegister ? 'email' : 'text'}
              className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
              placeholder={isRegister ? '请输入邮箱' : t('auth.usernamePlaceholder')}
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              required
              autoComplete="username"
            />
          </div>

          {isRegister && (
            <>
              <div>
                <label className="block text-sm text-gray-700 mb-1">邀请码</label>
                <input
                  type="text"
                  className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
                  placeholder="请输入邀请码"
                  value={inviteCode}
                  onChange={(event) => setInviteCode(event.target.value)}
                  required
                />
              </div>
              <div>
                <label className="block text-sm text-gray-700 mb-1">邮箱验证码</label>
                <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-2">
                  <input
                    type="text"
                    inputMode="numeric"
                    className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
                    placeholder="6 位验证码"
                    value={verificationCode}
                    onChange={(event) => setVerificationCode(event.target.value)}
                    required
                  />
                  <button
                    type="button"
                    className="border border-gray-300 rounded px-3 py-2 text-sm text-gray-700 disabled:opacity-60"
                    disabled={loading || codeSending}
                    onClick={() => void onSendCode()}
                  >
                    {codeSending ? '发送中' : '发送验证码'}
                  </button>
                </div>
              </div>
            </>
          )}
          <div>
            <label className="block text-sm text-gray-700 mb-1">{t('auth.password')}</label>
            <input
              type="password"
              className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
              placeholder={isRegister ? t('auth.passwordPlaceholderRegister') : t('auth.passwordPlaceholder')}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              required
              autoComplete={isRegister ? 'new-password' : 'current-password'}
            />
          </div>

          {isRegister && (
            <div>
              <label className="block text-sm text-gray-700 mb-1">{t('auth.confirmPassword')}</label>
              <input
                type="password"
                className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
                placeholder={t('auth.confirmPasswordPlaceholder')}
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                required
                autoComplete="new-password"
              />
            </div>
          )}

          {displayError && (
            <div className="text-sm text-red-600 bg-red-50 border border-red-200 rounded px-3 py-2">
              {displayError}
            </div>
          )}

          <button
            type="submit"
            className="w-full bg-blue-600 text-white rounded py-2 text-sm disabled:opacity-60"
            disabled={loading}
          >
            {loading ? t('common.loading') : submitLabel}
          </button>
        </form>
      </div>
    </div>
  );
}
