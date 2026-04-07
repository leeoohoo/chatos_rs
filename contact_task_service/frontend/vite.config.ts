import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
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
