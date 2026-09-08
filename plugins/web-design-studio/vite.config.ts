import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import vue from '@vitejs/plugin-vue';
import tailwindcss from '@tailwindcss/vite';
import AutoImport from 'unplugin-auto-import/vite';
import path from 'node:path';

export default defineConfig({
  define: {
    'process.env.NODE_ENV': JSON.stringify('production')
  },
  plugins: [
    react(),
    vue(),
    AutoImport({ imports: ['vue'], dts: false, include: [/library-runtime\/vendor\/inspira\/.*\.[jt]s$/, /library-runtime\/vendor\/inspira\/.*\.vue$/] }),
    tailwindcss()
  ],
  root: path.resolve(import.meta.dirname, 'ui-src'),
  resolve: {
    alias: {
      '@magic/registry/new-york/ui': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/magicui/components/ui'),
      '@magic': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/magicui'),
      '@shadcn-registry': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/shadcn'),
      '@antd-registry': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/antd'),
      '@chakra-registry': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/chakra'),
      'compositions': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/chakra/compositions'),
      '@spell': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/spell'),
      'cn': path.resolve(import.meta.dirname, 'ui-src/library-runtime/vendor/shadcn/cn.ts'),
      'next/link': path.resolve(import.meta.dirname, 'ui-src/library-runtime/compat/next-link.tsx'),
      'next/image': path.resolve(import.meta.dirname, 'ui-src/library-runtime/compat/next-image.tsx'),
      'next/form': path.resolve(import.meta.dirname, 'ui-src/library-runtime/compat/next-form.tsx'),
      'next/font/google': path.resolve(import.meta.dirname, 'ui-src/library-runtime/compat/next-font-google.ts')
    }
  },
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
    // The packaged studio may be serving this directory while a development
    // rebuild runs. Keeping the previous shell in place prevents lazy preview
    // iframes from receiving Express' ENOENT JSON while Vite prepares the new
    // assets.
    emptyOutDir: false,
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
