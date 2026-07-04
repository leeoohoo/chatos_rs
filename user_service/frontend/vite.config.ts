// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

function parsePort(rawValue: string | undefined, fallback: number): number {
  const parsed = Number.parseInt((rawValue || '').trim(), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

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
  const port = parsePort(env.USER_SERVICE_FRONTEND_PORT, 39191);
  const base = normalizeBasePath(env.VITE_BASE_PATH || env.USER_SERVICE_FRONTEND_BASE_PATH);
  const basePrefix = base === '/' ? '' : base.replace(/\/+$/, '');
  const proxyTarget =
    env.USER_SERVICE_API_PROXY_TARGET?.trim() ||
    env.VITE_API_PROXY_TARGET?.trim() ||
    'http://127.0.0.1:39190';

  return {
    base,
    plugins: [react()],
    build: {
      chunkSizeWarningLimit: 700,
    },
    server: {
      port,
      proxy: {
        '/api': {
          target: proxyTarget,
          changeOrigin: true,
        },
        ...(basePrefix
          ? {
              [`${basePrefix}/api`]: {
                target: proxyTarget,
                changeOrigin: true,
                rewrite: (path) => path.slice(basePrefix.length),
              },
            }
          : {}),
      },
    },
  };
});
