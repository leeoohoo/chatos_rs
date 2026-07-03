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
  const backendPort = parsePort(env.TASK_RUNNER_BACKEND_PORT || env.TASK_RUNNER_PORT, 39090);
  const apiProxyTarget =
    env.TASK_RUNNER_API_PROXY_TARGET?.trim() || `http://127.0.0.1:${backendPort}`;

  return {
    plugins: [react()],
    build: {
      chunkSizeWarningLimit: 700,
      rollupOptions: {
        output: {
          manualChunks(id) {
            if (!id.includes('node_modules')) {
              return undefined;
            }
            const packageName = resolvePackageName(id);
            if (!packageName) {
              return 'vendor';
            }
            if (reactVendorPackages.has(packageName)) {
              return 'react-vendor';
            }
            if (packageName.startsWith('@tanstack/')) {
              return 'query-vendor';
            }
            if (packageName === 'antd') {
              return 'antd-vendor';
            }
            if (
              packageName.startsWith('@ant-design/') ||
              packageName.startsWith('@rc-component/') ||
              packageName.startsWith('rc-')
            ) {
              return 'antd-support-vendor';
            }
            return 'vendor';
          },
        },
      },
    },
    server: {
      port: 39091,
      proxy: {
        '/api': {
          target: apiProxyTarget,
          changeOrigin: true,
        },
      },
    },
  };
});

const reactVendorPackages = new Set([
  '@remix-run/router',
  'react',
  'react-dom',
  'react-is',
  'react-router',
  'react-router-dom',
  'scheduler',
  'use-sync-external-store',
]);

function resolvePackageName(id: string): string | undefined {
  const marker = 'node_modules/';
  const markerIndex = id.lastIndexOf(marker);
  if (markerIndex === -1) {
    return undefined;
  }
  const parts = id.slice(markerIndex + marker.length).split('/');
  if (!parts[0]) {
    return undefined;
  }
  if (parts[0].startsWith('@')) {
    return parts[1] ? `${parts[0]}/${parts[1]}` : parts[0];
  }
  return parts[0];
}
