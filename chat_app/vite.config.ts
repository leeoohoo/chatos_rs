import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  build: {
    sourcemap: false,
    chunkSizeWarningLimit: 600,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules')) {
            if (
              id.includes('/mermaid/') ||
              id.includes('/cytoscape/')
            ) {
              return 'vendor-mermaid';
            }
            if (
              id.includes('/katex/') ||
              id.includes('/remark-math/') ||
              id.includes('/rehype-katex/')
            ) {
              return 'vendor-katex';
            }
            if (
              id.includes('/react-markdown/') ||
              id.includes('/remark-gfm/') ||
              id.includes('/rehype-highlight/') ||
              id.includes('/highlight.js/')
            ) {
              return 'vendor-markdown';
            }
            if (
              id.includes('/@xterm/xterm/') ||
              id.includes('/@xterm/addon-fit/')
            ) {
              return 'vendor-xterm';
            }
            return 'vendor-core';
          }
          return undefined;
        },
      },
    },
  },
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
    host: '0.0.0.0',
    port: 8088,
    open: true,
    proxy: {
      '/api': {
        target: 'http://localhost:3997',
        changeOrigin: true,
        ws: true,
      },
    },
  },

});
