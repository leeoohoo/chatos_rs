// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const matchesAny = (id: string, patterns: string[]): boolean => (
  patterns.some((pattern) => id.includes(pattern))
);

const MERMAID_CYTOSCAPE_PATTERNS = [
  '/cytoscape/',
  '/cytoscape-cose-bilkent/',
  '/cytoscape-fcose/',
];

const MERMAID_RUNTIME_PATTERNS = [
  '/mermaid/dist/mermaid.core.mjs',
  '/mermaid/dist/chunks/mermaid.core/',
];

const MERMAID_PARSER_PATTERNS = [
  '/@mermaid-js/parser/',
];

const MERMAID_LANGIUM_PATTERNS = [
  '/langium/',
  '/chevrotain/',
  '/chevrotain-allstar/',
];

const MERMAID_LSP_PATTERNS = [
  '/vscode-languageserver/',
  '/vscode-languageserver-protocol/',
  '/vscode-languageserver-types/',
  '/vscode-languageserver-textdocument/',
  '/vscode-jsonrpc/',
  '/vscode-uri/',
];

const MERMAID_GRAPH_PATTERNS = [
  '/d3/',
  '/d3-',
  '/dagre-d3-es/',
  '/roughjs/',
  '/@upsetjs/venn.js/',
];

const MERMAID_SUPPORT_PATTERNS = [
  '/dayjs/',
  '/dompurify/',
  '/khroma/',
  '/lodash-es/',
  '/marked/',
  '/stylis/',
  '/ts-dedent/',
  '/@braintree/sanitize-url/',
  '/@iconify/',
];

const MARKDOWN_HIGHLIGHT_PATTERNS = [
  '/highlight.js/',
  '/lowlight/',
];

const MARKDOWN_CORE_PATTERNS = [
  '/react-markdown/',
  '/remark-gfm/',
  '/remark-parse/',
  '/remark-rehype/',
  '/rehype-highlight/',
  '/unified/',
  '/vfile/',
  '/mdast-util-',
  '/hast-util-',
  '/micromark',
  '/unist-util-',
  '/html-url-attributes/',
  '/property-information/',
  '/space-separated-tokens/',
  '/comma-separated-tokens/',
  '/decode-named-character-reference/',
  '/character-entities/',
  '/character-entities-html4/',
  '/character-entities-legacy/',
  '/devlop/',
  '/trough/',
  '/bail/',
  '/zwitch/',
];

const REACT_CORE_PATTERNS = [
  '/react/',
  '/react-dom/',
  '/scheduler/',
  '/use-sync-external-store/',
];

const STATE_PATTERNS = [
  '/zustand/',
  '/immer/',
];

const UI_PATTERNS = [
  '/@radix-ui/',
  '/@headlessui/',
  '/framer-motion/',
  '/@floating-ui/',
  '/react-remove-scroll/',
  '/react-remove-scroll-bar/',
  '/react-style-singleton/',
  '/use-callback-ref/',
  '/use-sidecar/',
];

const SDK_PATTERNS = [
  '/openai/',
  '/@modelcontextprotocol/sdk/',
];

const UTILITY_PATTERNS = [
  '/axios/',
  '/clsx/',
  '/tailwind-merge/',
  '/class-variance-authority/',
  '/date-fns/',
  '/uuid/',
  '/lucide-react/',
];

const DEFERRED_HTML_PRELOAD_CHUNKS = [
  'vendor-highlight-',
  'vendor-katex-',
  'vendor-mermaid-',
];

const HIGHLIGHT_LANGUAGE_OPTIMIZE_DEPS = [
  'highlight.js/lib/core',
  'highlight.js/lib/languages/bash',
  'highlight.js/lib/languages/c',
  'highlight.js/lib/languages/cmake',
  'highlight.js/lib/languages/cpp',
  'highlight.js/lib/languages/csharp',
  'highlight.js/lib/languages/css',
  'highlight.js/lib/languages/dart',
  'highlight.js/lib/languages/dockerfile',
  'highlight.js/lib/languages/dos',
  'highlight.js/lib/languages/go',
  'highlight.js/lib/languages/gradle',
  'highlight.js/lib/languages/graphql',
  'highlight.js/lib/languages/ini',
  'highlight.js/lib/languages/java',
  'highlight.js/lib/languages/javascript',
  'highlight.js/lib/languages/json',
  'highlight.js/lib/languages/kotlin',
  'highlight.js/lib/languages/less',
  'highlight.js/lib/languages/lua',
  'highlight.js/lib/languages/makefile',
  'highlight.js/lib/languages/markdown',
  'highlight.js/lib/languages/objectivec',
  'highlight.js/lib/languages/php',
  'highlight.js/lib/languages/plaintext',
  'highlight.js/lib/languages/powershell',
  'highlight.js/lib/languages/protobuf',
  'highlight.js/lib/languages/python',
  'highlight.js/lib/languages/r',
  'highlight.js/lib/languages/ruby',
  'highlight.js/lib/languages/rust',
  'highlight.js/lib/languages/scala',
  'highlight.js/lib/languages/scss',
  'highlight.js/lib/languages/sql',
  'highlight.js/lib/languages/swift',
  'highlight.js/lib/languages/typescript',
  'highlight.js/lib/languages/xml',
  'highlight.js/lib/languages/yaml',
];

// https://vitejs.dev/config/
export default defineConfig({
  // Relative assets keep both direct local access and the gateway's /app/ mount working.
  base: './',
  plugins: [react()],
  build: {
    modulePreload: {
      resolveDependencies: (_filename, dependencies, context) => (
        context.hostType === 'html'
          ? dependencies.filter((dependency) => (
              !DEFERRED_HTML_PRELOAD_CHUNKS.some((chunk) => dependency.includes(chunk))
            ))
          : dependencies
      ),
    },
    sourcemap: false,
    chunkSizeWarningLimit: 600,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules')) {
            if (matchesAny(id, MERMAID_CYTOSCAPE_PATTERNS)) {
              return 'vendor-mermaid-cytoscape';
            }
            if (matchesAny(id, MERMAID_RUNTIME_PATTERNS)) {
              return undefined;
            }
            if (matchesAny(id, MERMAID_PARSER_PATTERNS)) {
              return 'vendor-mermaid-parser';
            }
            if (matchesAny(id, MERMAID_LANGIUM_PATTERNS)) {
              return 'vendor-mermaid-langium';
            }
            if (matchesAny(id, MERMAID_LSP_PATTERNS)) {
              return 'vendor-mermaid-lsp';
            }
            if (matchesAny(id, MERMAID_GRAPH_PATTERNS)) {
              return 'vendor-mermaid-graph';
            }
            if (matchesAny(id, MERMAID_SUPPORT_PATTERNS)) {
              return 'vendor-mermaid-support';
            }
            if (
              id.includes('/katex/') ||
              id.includes('/remark-math/') ||
              id.includes('/rehype-katex/')
            ) {
              return 'vendor-katex';
            }
            if (matchesAny(id, MARKDOWN_HIGHLIGHT_PATTERNS)) {
              return 'vendor-highlight';
            }
            if (matchesAny(id, MARKDOWN_CORE_PATTERNS)) {
              return 'vendor-markdown-core';
            }
            if (
              id.includes('/@xterm/xterm/') ||
              id.includes('/@xterm/addon-fit/')
            ) {
              return 'vendor-xterm';
            }
            if (matchesAny(id, REACT_CORE_PATTERNS)) {
              return 'vendor-react';
            }
            if (matchesAny(id, STATE_PATTERNS)) {
              return 'vendor-state';
            }
            if (matchesAny(id, UI_PATTERNS)) {
              return 'vendor-ui';
            }
            if (matchesAny(id, SDK_PATTERNS)) {
              return 'vendor-sdk';
            }
            if (matchesAny(id, UTILITY_PATTERNS)) {
              return 'vendor-utils';
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
      ...HIGHLIGHT_LANGUAGE_OPTIMIZE_DEPS,
    ],
    exclude: ['fs', 'path', 'crypto'],
  },
  define: {
    global: 'globalThis',
  },
  server: {
    host: '0.0.0.0',
    port: 8088,
    strictPort: true,
    open: true,
    proxy: {
      '/api/chatos': {
        target: 'http://localhost:3997',
        changeOrigin: true,
        ws: true,
        rewrite: (path) => path.replace(/^\/api\/chatos/, '/api'),
      },
      '/api': {
        target: 'http://localhost:3997',
        changeOrigin: true,
        ws: true,
      },
    },
  },

});
