// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

function normalizeBasePath(rawValue: string | undefined): string {
  const value = (rawValue || '').trim();
  if (!value || value === '/') {
    return '/';
  }
  const withLeadingSlash = value.startsWith('/') ? value : `/${value}`;
  return withLeadingSlash.endsWith('/') ? withLeadingSlash : `${withLeadingSlash}/`;
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
        '/api': target,
        '/health': target,
        ...(basePrefix
          ? {
              [`${basePrefix}/api`]: {
                target,
                changeOrigin: true,
                rewrite: (path) => path.slice(basePrefix.length),
              },
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
