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

function trimmedEnv(env: Record<string, string>, key: string): string | undefined {
  const value = env[key]?.trim();
  return value || undefined;
}

function sandboxAuthHeaders(env: Record<string, string>): Record<string, string> | undefined {
  const clientId =
    trimmedEnv(env, 'SANDBOX_MANAGER_API_PROXY_CLIENT_ID') ||
    trimmedEnv(env, 'SANDBOX_MANAGER_SYSTEM_CLIENT_ID') ||
    trimmedEnv(env, 'TASK_RUNNER_SANDBOX_MANAGER_CLIENT_ID');
  const clientKey =
    trimmedEnv(env, 'SANDBOX_MANAGER_API_PROXY_CLIENT_KEY') ||
    trimmedEnv(env, 'SANDBOX_MANAGER_SYSTEM_CLIENT_KEY') ||
    trimmedEnv(env, 'TASK_RUNNER_SANDBOX_MANAGER_CLIENT_KEY');
  if (clientId && clientKey) {
    return {
      'x-sandbox-client-id': clientId,
      'x-sandbox-client-key': clientKey,
    };
  }

  const operatorToken =
    trimmedEnv(env, 'SANDBOX_MANAGER_API_PROXY_OPERATOR_TOKEN') ||
    trimmedEnv(env, 'SANDBOX_MANAGER_OPERATOR_TOKEN');
  return operatorToken ? { 'x-sandbox-operator-token': operatorToken } : undefined;
}

function sandboxApiProxy(
  target: string,
  env: Record<string, string>,
  rewrite?: ProxyOptions['rewrite'],
): ProxyOptions {
  const headers = sandboxAuthHeaders(env);
  return {
    target,
    changeOrigin: true,
    ...(headers ? { headers } : {}),
    ...(rewrite ? { rewrite } : {}),
  };
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const base = normalizeBasePath(env.VITE_BASE_PATH || env.SANDBOX_MANAGER_FRONTEND_BASE_PATH);
  const target = env.SANDBOX_MANAGER_API_PROXY_TARGET || 'http://127.0.0.1:8095';
  const basePrefix = base === '/' ? '' : base.replace(/\/+$/, '');

  return {
    base,
    plugins: [react()],
    server: {
      host: '0.0.0.0',
      port: Number(env.SANDBOX_MANAGER_FRONTEND_PORT || 8096),
      proxy: {
        '/api': sandboxApiProxy(target, env),
        '/health': target,
        ...(basePrefix
          ? {
              [`${basePrefix}/api`]: sandboxApiProxy(target, env, (path) =>
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
