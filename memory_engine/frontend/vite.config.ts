// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import { basePrefixFromBase, createBasePathProxy, normalizeBasePath, parsePort } from '../../scripts/frontend/viteShared';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const frontendPort = parsePort(env.MEMORY_ENGINE_FRONTEND_PORT, 4178);
  const memoryEnginePort = parsePort(env.MEMORY_ENGINE_PORT, 7081);
  const userServicePort = parsePort(env.USER_SERVICE_PORT, 39190);
  const base = normalizeBasePath(env.VITE_BASE_PATH || env.MEMORY_ENGINE_FRONTEND_BASE_PATH);
  const basePrefix = basePrefixFromBase(base);
  const memoryEngineProxyTarget =
    env.MEMORY_ENGINE_API_PROXY_TARGET?.trim() || `http://127.0.0.1:${memoryEnginePort}`;
  const userServiceProxyTarget =
    env.USER_SERVICE_API_PROXY_TARGET?.trim() || `http://127.0.0.1:${userServicePort}`;

  return {
    base,
    plugins: mode === 'test' ? [] : [react()],
    server: {
      port: frontendPort,
      host: '0.0.0.0',
      proxy: {
        '/api/memory-engine': {
          target: memoryEngineProxyTarget,
          changeOrigin: true,
        },
        '/user-service/api': {
          target: userServiceProxyTarget,
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/user-service/, ''),
        },
        '/api': {
          target: userServiceProxyTarget,
          changeOrigin: true,
        },
        ...createBasePathProxy(basePrefix, '/api/memory-engine', memoryEngineProxyTarget),
        ...createBasePathProxy(basePrefix, '/api', userServiceProxyTarget),
        ...createBasePathProxy(basePrefix, '/health', memoryEngineProxyTarget),
      },
    },
    test: {
      environment: 'jsdom',
      globals: true,
      setupFiles: './src/test/setup.ts',
    },
  };
});
