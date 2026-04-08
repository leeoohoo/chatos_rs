import type { Config } from 'drizzle-kit';

export default {
  schema: './src/lib/database/schema.ts',
  out: './drizzle',
  driver: 'better-sqlite',
  dbCredentials: {
    url: './agent_workspace.db',
  },
  verbose: true,
  strict: true,
} satisfies Config;
