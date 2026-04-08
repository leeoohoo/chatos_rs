import React from 'react';
import { useAuthStore } from '@/lib/auth/authStore';

export function AuthPanel() {
  const { login, loading, error, clearError } = useAuthStore();
  const [username, setUsername] = React.useState('');
  const [password, setPassword] = React.useState('');

  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    clearError();
    try {
      await login(username, password);
    } catch {
      // 统一由 auth store 维护错误提示
    }
  };

  return (
    <div className="min-h-screen bg-gray-100 flex items-center justify-center p-4">
      <div className="w-full max-w-md bg-white rounded-lg shadow border border-gray-200 p-6">
        <h1 className="text-xl font-semibold text-gray-900 mb-1">
          登录
        </h1>
        <p className="text-sm text-gray-500 mb-5">使用 memory_server 账号继续</p>

        <form onSubmit={onSubmit} className="space-y-3">
          <div>
            <label className="block text-sm text-gray-700 mb-1">用户名</label>
            <input
              type="text"
              className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
              placeholder="请输入用户名"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              required
            />
          </div>
          <div>
            <label className="block text-sm text-gray-700 mb-1">密码</label>
            <input
              type="password"
              className="w-full border border-gray-300 rounded px-3 py-2 text-sm"
              placeholder="请输入密码"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
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
            {loading ? '处理中...' : '登录'}
          </button>
        </form>
      </div>
    </div>
  );
}
