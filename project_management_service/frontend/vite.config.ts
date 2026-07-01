// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 39211,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:39210',
        changeOrigin: true,
      },
    },
  },
});
