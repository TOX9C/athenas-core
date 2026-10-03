import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: ['tests/**/*.test.ts', 'packages/*/tests/**/*.test.ts', 'packages/*/test/**/*.test.ts', 'plugins/**/*.test.ts'],
    coverage: {
      provider: 'v8',
      // Scope is intentionally mcp-server-only (`npm run test:coverage`);
      // plugins/ and tests/ have no coverage targets yet.
      include: ['packages/mcp-server/src/**/*.ts'],
      exclude: ['**/node_modules/**', '**/dist/**'],
    },
  },
})
