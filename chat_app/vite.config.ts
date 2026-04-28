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

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  build: {
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
  server: {
    host: '0.0.0.0',
    port: 8088,
    open: true,
    proxy: {
      '/api': {
        target: 'http://localhost:3997',
        changeOrigin: true,
        ws: true,
      },
    },
  },

});
