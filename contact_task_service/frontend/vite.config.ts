import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

function manualChunks(id: string): string | undefined {
  if (!id.includes('node_modules/')) {
    return undefined;
  }

  if (
    id.includes('/node_modules/react/')
    || id.includes('/node_modules/react-dom/')
    || id.includes('/node_modules/scheduler/')
  ) {
    return 'vendor-react';
  }

  if (
    id.includes('/node_modules/@emotion/')
  ) {
    return 'vendor-emotion';
  }

  return 'vendor-antd';
}

export default defineConfig({
  plugins: [react()],
  build: {
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      output: {
        manualChunks,
      },
    },
  },
  server: {
    port: 5177,
    host: true,
    proxy: {
      '/api/memory/v1': {
        target: 'http://127.0.0.1:7080',
        changeOrigin: true,
      },
      '/api/task-service/v1': {
        target: 'http://127.0.0.1:8096',
        changeOrigin: true,
      },
    },
  },
});
