import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    globals: true,
    environment: 'node',
    include: [
      'tests/**/*.test.ts',
      'packages/*/tests/**/*.test.ts',
      'electron/**/__tests__/**/*.test.ts',
    ],
    coverage: {
      provider: 'v8',
      include: ['electron/**/*.ts', 'packages/mcp-server/src/**/*.ts'],
      exclude: ['**/node_modules/**', '**/dist/**'],
    },
  },
})
