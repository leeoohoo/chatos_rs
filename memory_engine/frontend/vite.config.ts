// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

function parsePort(rawValue: string | undefined, fallback: number): number {
  const parsed = Number.parseInt((rawValue || '').trim(), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const memoryEnginePort = parsePort(env.MEMORY_ENGINE_PORT, 7081);
  const userServicePort = parsePort(env.USER_SERVICE_PORT, 39190);
  const memoryEngineProxyTarget =
    env.MEMORY_ENGINE_API_PROXY_TARGET?.trim() || `http://127.0.0.1:${memoryEnginePort}`;
  const userServiceProxyTarget =
    env.USER_SERVICE_API_PROXY_TARGET?.trim() || `http://127.0.0.1:${userServicePort}`;

  return {
    plugins: mode === 'test' ? [] : [react()],
    server: {
      port: 4178,
      host: '0.0.0.0',
      proxy: {
        '/api/memory-engine': {
          target: memoryEngineProxyTarget,
          changeOrigin: true,
        },
        '/api': {
          target: userServiceProxyTarget,
          changeOrigin: true,
        },
      },
    },
    test: {
      environment: 'jsdom',
      globals: true,
      setupFiles: './src/test/setup.ts',
    },
  };
});
