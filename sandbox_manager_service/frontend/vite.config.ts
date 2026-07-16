// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv, type ProxyOptions } from 'vite';
import react from '@vitejs/plugin-react';

function normalizeBasePath(rawValue: string | undefined): string {
  const value = (rawValue || '').trim();
  if (!value || value === '/') {
    return '/';
  }
  const withLeadingSlash = value.startsWith('/') ? value : `/${value}`;
  return withLeadingSlash.endsWith('/') ? withLeadingSlash : `${withLeadingSlash}/`;
}

function sandboxApiProxy(
  target: string,
  operatorToken: string,
  rewrite?: ProxyOptions['rewrite'],
): ProxyOptions {
  return {
    target,
    changeOrigin: true,
    headers: operatorToken
      ? { 'x-sandbox-operator-token': operatorToken }
      : undefined,
    ...(rewrite ? { rewrite } : {}),
  };
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const base = normalizeBasePath(env.VITE_BASE_PATH || env.SANDBOX_MANAGER_FRONTEND_BASE_PATH);
  const target = env.SANDBOX_MANAGER_API_PROXY_TARGET || 'http://127.0.0.1:8095';
  const operatorToken =
    env.SANDBOX_MANAGER_API_PROXY_OPERATOR_TOKEN ||
    env.SANDBOX_MANAGER_OPERATOR_TOKEN ||
    ((env.CHATOS_ENV || 'local').trim().toLowerCase() === 'local'
      ? 'chatos-sandbox-manager-dev-operator-token'
      : '');
  const basePrefix = base === '/' ? '' : base.replace(/\/+$/, '');

  return {
    base,
    plugins: [react()],
    server: {
      host: '0.0.0.0',
      port: Number(env.SANDBOX_MANAGER_FRONTEND_PORT || 8096),
      proxy: {
        '/api': sandboxApiProxy(target, operatorToken),
        '/health': target,
        ...(basePrefix
          ? {
              [`${basePrefix}/api`]: sandboxApiProxy(target, operatorToken, (path) =>
                path.slice(basePrefix.length),
              ),
              [`${basePrefix}/health`]: {
                target,
                changeOrigin: true,
                rewrite: (path) => path.slice(basePrefix.length),
              },
            }
          : {}),
      },
    },
  };
});
