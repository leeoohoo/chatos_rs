// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  build: {
    // Page-level lazy loading keeps feature code split; the remaining shared
    // Ant Design/runtime chunk is intentionally reused across every admin page.
    chunkSizeWarningLimit: 700,
  },
  server: {
    host: '127.0.0.1',
    port: 39261,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:39260',
        changeOrigin: true,
      },
    },
  },
});
