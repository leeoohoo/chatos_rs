import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': '/src',
      '@/components': '/src/components',
      '@/hooks': '/src/hooks',
      '@/lib': '/src/lib',
      '@/types': '/src/types',
      '@/styles': '/src/styles',
    },
  },

  optimizeDeps: {
    include: [
      'react',
      'react-dom',
      'zustand',
    ],
    exclude: [
      'better-sqlite3',
      'drizzle-orm',
      'fs',
      'path',
      'crypto',
    ],
  },
  define: {
    global: 'globalThis',
  },
  server: {
    port: 5173,
    open: true,
    proxy: {
      '/api': {
        target: 'http://localhost:3001',
        changeOrigin: true,
      },
    },
  },

});