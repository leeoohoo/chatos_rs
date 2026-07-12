// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const target = env.LOCAL_CONNECTOR_CORE_API_PROXY_TARGET || 'http://127.0.0.1:39232';
  const desktopAuthToken = env.LOCAL_CONNECTOR_DESKTOP_AUTH_TOKEN?.trim();

  return {
    base: mode === 'electron' ? './' : '/',
    plugins: [react()],
    server: {
      host: '127.0.0.1',
      port: Number(env.LOCAL_CONNECTOR_CLIENT_FRONTEND_PORT || 39233),
      proxy: {
        '/api': {
          target,
          changeOrigin: true,
          headers: desktopAuthToken
            ? { Authorization: `Bearer ${desktopAuthToken}` }
            : undefined,
        },
      },
    },
  };
});
