import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';
import path from 'node:path';

export default defineConfig({
  plugins: [react(), vue(), tailwindcss()],
  root: path.resolve(import.meta.dirname, 'ui-src'),
  base: './',
  server: {
    host: '127.0.0.1',
    port: 4187,
    strictPort: true,
    proxy: {
      '/api': 'http://127.0.0.1:4188'
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
