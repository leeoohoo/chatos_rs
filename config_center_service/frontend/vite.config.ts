import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    host: '0.0.0.0',
    port: 39271,
    proxy: {
      '/api': 'http://127.0.0.1:39270',
      '/health': 'http://127.0.0.1:39270',
      '/ready': 'http://127.0.0.1:39270',
    },
  },
});
