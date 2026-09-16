import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  root: 'ui-src',
  build: {
    outDir: '../ui',
    emptyOutDir: true,
    rollupOptions: { output: { entryFileNames: 'assets/app.js', chunkFileNames: 'assets/[name].js', assetFileNames: (asset) => asset.name?.endsWith('.css') ? 'assets/app.css' : 'assets/[name][extname]' } }
  },
  server: { port: 4197, proxy: { '/api': 'http://127.0.0.1:4198' } }
});
