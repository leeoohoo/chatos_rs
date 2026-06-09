import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  build: {
    chunkSizeWarningLimit: 700,
  },
  server: {
    port: 39091,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:39090',
        changeOrigin: true,
      },
    },
  },
});
