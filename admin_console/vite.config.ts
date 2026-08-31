import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const proxyTarget = process.env.CHATOS_ADMIN_DEV_GATEWAY || 'http://127.0.0.1:9080';
const proxyHost = process.env.CHATOS_ADMIN_DEV_GATEWAY_HOST || 'admin.jgoool.com';

export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    port: 39200,
    strictPort: true,
    proxy: {
      '/api/admin': {
        target: proxyTarget,
        changeOrigin: true,
        headers: { Host: proxyHost },
      },
    },
  },
  build: {
    chunkSizeWarningLimit: 900,
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [
            { name: 'react-vendor', test: /node_modules\/(react|react-dom|react-router-dom)/ },
            { name: 'query-vendor', test: /node_modules\/@tanstack/ },
          ],
        },
      },
    },
  },
});
