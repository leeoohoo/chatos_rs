import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: './src/modules/memory-engine/test/setup.ts',
    include: ['src/**/*.test.{ts,tsx}'],
  },
});
