const js = require('@eslint/js')
const tseslint = require('typescript-eslint')
const reactPlugin = require('eslint-plugin-react')
const reactHooksPlugin = require('eslint-plugin-react-hooks')
const reactRefreshPlugin = require('eslint-plugin-react-refresh')
const prettierPlugin = require('eslint-plugin-prettier')
const prettierConfig = require('eslint-config-prettier')

module.exports = tseslint.config(
  {
    ignores: [
      'out/**',
      'dist/**',
      'frontend/dist/**',
      'frontend/public/**',
      'frontend/vendor/**',
      'e2e-tests/**',
      'dist-electron/**',
      'node_modules/**',
      'target/**',
      'patches/**',
      'packages/mcp-server/dist/**',
      'packages/mcp-server/node_modules/**',
      '*.rej',
      '*.orig',
      'fix_*.js',
      'fix-*.js',
      'patch_*.js',
      'patch-*.js',
      'update_*.ts',
      'update_*.sh',
      'update-*.js',
      'update-*.patch',
      'update-*.ts',
      'test-orchestrator.ts',
      'test-*.js',
      'apply-*.patch',
      'bin/**',
      'eslint.config.js',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  prettierConfig,
  {
    files: ['**/*.{ts,tsx}'],
    plugins: {
      '@typescript-eslint': tseslint.plugin,
      react: reactPlugin,
      'react-hooks': reactHooksPlugin,
      'react-refresh': reactRefreshPlugin.reactRefresh.plugin,
      prettier: prettierPlugin,
    },
    languageOptions: {
      parser: tseslint.parser,
      parserOptions: {
        ecmaVersion: 'latest',
        sourceType: 'module',
        ecmaFeatures: { jsx: true },
      },
      globals: {
        window: 'readonly',
        document: 'readonly',
        console: 'readonly',
        process: 'readonly',
        __dirname: 'readonly',
        __filename: 'readonly',
        module: 'readonly',
        require: 'readonly',
        exports: 'writable',
        global: 'readonly',
        Buffer: 'readonly',
        setTimeout: 'readonly',
        setInterval: 'readonly',
        clearTimeout: 'readonly',
        clearInterval: 'readonly',
        setImmediate: 'readonly',
        clearImmediate: 'readonly',
        NodeJS: 'readonly',
      },
    },
    settings: {
      react: {
        version: 'detect',
      },
    },
    rules: {
      ...reactPlugin.configs.recommended.rules,
      ...reactPlugin.configs['jsx-runtime'].rules,
      'react-hooks/rules-of-hooks': 'error',
      'react-hooks/exhaustive-deps': 'warn',
      'react-refresh/only-export-components': [
        'warn',
        { allowConstantExport: true },
      ],
      'react/react-in-jsx-scope': 'off',
      'react/prop-types': 'off',
      '@typescript-eslint/no-unused-vars': [
        'warn',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/no-non-null-assertion': 'off',
      '@typescript-eslint/no-unsafe-function-type': 'warn',
      'no-empty': ['error', { allowEmptyCatch: true }],
      'prettier/prettier': 'error',
    },
  },
  {
    files: ['electron/**/*.ts', 'plugins/**/*.ts'],
    rules: {
      'react-refresh/only-export-components': 'off',
    },
  },
  {
    files: ['packages/mcp-server/**/*.ts'],
    rules: {
      'react-refresh/only-export-components': 'off',
      'react/no-unescaped-entities': 'off',
    },
  },
  {
    files: ['src/utils/commandParser.ts', 'src/utils/ansi.ts'],
    rules: {
      'no-useless-escape': 'off',
      'no-control-regex': 'off',
    },
  },
  {
    files: ['electron/**/*.ts'],
    rules: {
      'no-control-regex': 'off',
      '@typescript-eslint/no-require-imports': 'warn',
    },
  },
  {
    files: ['src/**/*.{ts,tsx}'],
    rules: {
      'react/no-unescaped-entities': 'off',
      'no-control-regex': 'off',
    },
  },
  {
    files: ['**/*.ts', '**/*.tsx'],
    rules: {
      '@typescript-eslint/ban-ts-comment': 'warn',
    },
  },
)
