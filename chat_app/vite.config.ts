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
    id.includes('/node_modules/mermaid/')
  ) {
    return 'vendor-mermaid';
  }

  if (
    id.includes('/node_modules/@mermaid-js/')
    || id.includes('/node_modules/langium/')
    || id.includes('/node_modules/chevrotain/')
  ) {
    return 'vendor-mermaid-parser';
  }

  if (
    id.includes('/node_modules/d3/')
    || id.includes('/node_modules/d3-')
    || id.includes('/node_modules/internmap/')
    || id.includes('/node_modules/delaunator/')
    || id.includes('/node_modules/robust-predicates/')
  ) {
    return 'vendor-d3';
  }

  if (
    id.includes('/node_modules/cytoscape/')
    || id.includes('/node_modules/cytoscape-')
    || id.includes('/node_modules/cose-base/')
    || id.includes('/node_modules/layout-base/')
  ) {
    return 'vendor-cytoscape';
  }

  if (id.includes('/node_modules/katex/')) {
    return 'vendor-katex';
  }

  if (
    id.includes('/node_modules/@xterm/')
    || id.includes('/node_modules/xterm/')
  ) {
    return 'vendor-xterm';
  }

  if (id.includes('/node_modules/highlight.js/')) {
    return 'vendor-highlight';
  }

  return 'vendor-misc';
}

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
  build: {
    chunkSizeWarningLimit: 1600,
    rollupOptions: {
      output: {
        manualChunks,
      },
    },
  },
  server: {
    host: '0.0.0.0',
    port: 8088,
    open: true,
    proxy: {
      '/api': {
        target: 'http://localhost:3001',
        changeOrigin: true,
        ws: true,
      },
    },
  },

});
