import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '');
  const target = env.SANDBOX_MANAGER_API_PROXY_TARGET || 'http://127.0.0.1:8095';

  return {
    plugins: [react()],
    server: {
      host: '0.0.0.0',
      port: Number(env.SANDBOX_MANAGER_FRONTEND_PORT || 8096),
      proxy: {
        '/api': target,
        '/health': target,
      },
    },
  };
});
