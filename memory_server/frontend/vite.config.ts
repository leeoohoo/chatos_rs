import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  build: {
    sourcemap: false,
    chunkSizeWarningLimit: 550,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) {
            return undefined;
          }
          if (id.includes('/antd/') || id.includes('/@ant-design/')) {
            return 'vendor-antd';
          }
          if (id.includes('/axios/')) {
            return 'vendor-axios';
          }
          return undefined;
        },
      },
    },
  },
  server: {
    port: 5176,
    host: true,
  },
});
