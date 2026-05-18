import React from 'react';
import { useAuthStore } from '@/lib/auth/authStore';
import { useI18n } from '../i18n/I18nProvider';

type AuthMode = 'login' | 'register';

export function AuthPanel() {
  const { login, register, loading, error, clearError } = useAuthStore();
  const { t } = useI18n();
  const [mode, setMode] = React.useState<AuthMode>('login');
  const [username, setUsername] = React.useState('');
  const [password, setPassword] = React.useState('');
  const [confirmPassword, setConfirmPassword] = React.useState('');
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
    resetErrors();
  }, [resetErrors]);

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
        setLocalError(t('auth.usernameMinLength'));
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
        await register(trimmedUsername, password);
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
            <label className="block text-sm text-gray-700 mb-1">{t('auth.username')}</label>
            <input
              type="text"
              className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
              placeholder={isRegister ? t('auth.usernamePlaceholderRegister') : t('auth.usernamePlaceholder')}
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              required
              autoComplete="username"
            />
          </div>
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
