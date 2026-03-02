import React from 'react';
import { useAuthStore } from '@/lib/auth/authStore';

type Mode = 'login' | 'register';

export function AuthPanel() {
  const { login, register, loading, error, clearError } = useAuthStore();
  const [mode, setMode] = React.useState<Mode>('login');
  const [email, setEmail] = React.useState('');
  const [password, setPassword] = React.useState('');
  const [displayName, setDisplayName] = React.useState('');

  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    clearError();
    try {
      if (mode === 'login') {
        await login(email, password);
        return;
      }
      await register(email, password, displayName || undefined);
    } catch {
      // 统一由 auth store 维护错误提示
    }
  };

  return (
    <div className="min-h-screen bg-gray-100 flex items-center justify-center p-4">
      <div className="w-full max-w-md bg-white rounded-lg shadow border border-gray-200 p-6">
        <h1 className="text-xl font-semibold text-gray-900 mb-1">
          {mode === 'login' ? '登录' : '注册'}
        </h1>
        <p className="text-sm text-gray-500 mb-5">继续使用聊天系统</p>

        <div className="flex rounded-md border border-gray-200 overflow-hidden mb-4">
          <button
            type="button"
            className={`flex-1 py-2 text-sm ${mode === 'login' ? 'bg-blue-600 text-white' : 'bg-white text-gray-600'}`}
            onClick={() => {
              clearError();
              setMode('login');
            }}
          >
            登录
          </button>
          <button
            type="button"
            className={`flex-1 py-2 text-sm ${mode === 'register' ? 'bg-blue-600 text-white' : 'bg-white text-gray-600'}`}
            onClick={() => {
              clearError();
              setMode('register');
            }}
          >
            注册
          </button>
        </div>

        <form onSubmit={onSubmit} className="space-y-3">
          <div>
            <label className="block text-sm text-gray-700 mb-1">邮箱</label>
            <input
              type="email"
              className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
              placeholder="name@example.com"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              required
            />
          </div>
          {mode === 'register' && (
            <div>
              <label className="block text-sm text-gray-700 mb-1">昵称（可选）</label>
              <input
                type="text"
                className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
                placeholder="请输入昵称"
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
              />
            </div>
          )}
          <div>
            <label className="block text-sm text-gray-700 mb-1">密码</label>
            <input
              type="password"
              className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
              placeholder="至少 8 位"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              minLength={8}
              required
            />
          </div>

          {error && (
            <div className="text-sm text-red-600 bg-red-50 border border-red-200 rounded px-3 py-2">
              {error}
            </div>
          )}

          <button
            type="submit"
            className="w-full bg-blue-600 text-white rounded py-2 text-sm disabled:opacity-60"
            disabled={loading}
          >
            {loading ? '处理中...' : mode === 'login' ? '登录' : '注册并登录'}
          </button>
        </form>
      </div>
    </div>
  );
}
