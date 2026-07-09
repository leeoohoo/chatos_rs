// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import { basePrefixFromBase, createBasePathProxy, normalizeBasePath, parsePort } from '../../scripts/frontend/viteShared';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const port = parsePort(env.USER_SERVICE_FRONTEND_PORT, 39191);
  const base = normalizeBasePath(env.VITE_BASE_PATH || env.USER_SERVICE_FRONTEND_BASE_PATH);
  const basePrefix = basePrefixFromBase(base);
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
        ...createBasePathProxy(basePrefix, '/api', proxyTarget),
      },
    },
  };
});
