// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import { basePrefixFromBase, createBasePathProxy, normalizeBasePath, parsePort } from '../../scripts/frontend/viteShared';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const frontendPort = parsePort(env.PROJECT_SERVICE_FRONTEND_PORT, 39211);
  const backendPort = parsePort(env.PROJECT_SERVICE_PORT, 39210);
  const base = normalizeBasePath(env.VITE_BASE_PATH || env.PROJECT_SERVICE_FRONTEND_BASE_PATH);
  const basePrefix = basePrefixFromBase(base);
  const proxyTarget =
    env.PROJECT_SERVICE_API_PROXY_TARGET?.trim() ||
    env.VITE_API_PROXY_TARGET?.trim() ||
    `http://127.0.0.1:${backendPort}`;

  return {
    base,
    plugins: [react()],
    server: {
      port: frontendPort,
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
