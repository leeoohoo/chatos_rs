import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  build: {
    chunkSizeWarningLimit: 700,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) {
            return undefined;
          }
          const packageName = resolvePackageName(id);
          if (!packageName) {
            return 'vendor';
          }
          if (reactVendorPackages.has(packageName)) {
            return 'react-vendor';
          }
          if (packageName.startsWith('@tanstack/')) {
            return 'query-vendor';
          }
          if (packageName === 'antd') {
            return 'antd-vendor';
          }
          if (
            packageName.startsWith('@ant-design/') ||
            packageName.startsWith('@rc-component/') ||
            packageName.startsWith('rc-')
          ) {
            return 'antd-support-vendor';
          }
          return 'vendor';
        },
      },
    },
  },
  server: {
    port: 39091,
    proxy: {
      '/api': {
        target: 'http://127.0.0.1:39090',
        changeOrigin: true,
      },
    },
  },
});

const reactVendorPackages = new Set([
  '@remix-run/router',
  'react',
  'react-dom',
  'react-is',
  'react-router',
  'react-router-dom',
  'scheduler',
  'use-sync-external-store',
]);

function resolvePackageName(id: string): string | undefined {
  const marker = 'node_modules/';
  const markerIndex = id.lastIndexOf(marker);
  if (markerIndex === -1) {
    return undefined;
  }
  const parts = id.slice(markerIndex + marker.length).split('/');
  if (!parts[0]) {
    return undefined;
  }
  if (parts[0].startsWith('@')) {
    return parts[1] ? `${parts[0]}/${parts[1]}` : parts[0];
  }
  return parts[0];
}
