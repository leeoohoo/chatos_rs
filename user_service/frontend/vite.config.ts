import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

function parsePort(rawValue: string | undefined, fallback: number): number {
  const parsed = Number.parseInt((rawValue || '').trim(), 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const port = parsePort(env.USER_SERVICE_FRONTEND_PORT, 39191);
  const proxyTarget =
    env.USER_SERVICE_API_PROXY_TARGET?.trim() ||
    env.VITE_API_PROXY_TARGET?.trim() ||
    'http://127.0.0.1:39190';

  return {
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
      },
    },
  };
});
