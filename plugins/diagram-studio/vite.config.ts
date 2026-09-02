import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import path from 'node:path';

export default defineConfig({
  plugins: [react()],
  root: path.resolve(import.meta.dirname, 'ui-src'),
  base: './',
  resolve: {
    alias: {
      'elkjs/lib/elk.bundled.js': path.resolve(import.meta.dirname, 'ui-src/elk-browser.ts')
    }
  },
  server: {
    host: '127.0.0.1',
    port: 4177,
    strictPort: true,
    proxy: {
      '/api': 'http://127.0.0.1:4178'
    }
  },
  build: {
    outDir: path.resolve(import.meta.dirname, 'ui'),
    emptyOutDir: true,
    sourcemap: false,
    assetsInlineLimit: 4096,
    rollupOptions: {
      output: {
        entryFileNames: 'assets/app.js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: (assetInfo) => assetInfo.name?.endsWith('.css')
          ? 'assets/app.css'
          : 'assets/[name][extname]'
      }
    }
  }
});
