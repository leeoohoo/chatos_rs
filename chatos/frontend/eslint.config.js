// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import tseslint from '@typescript-eslint/eslint-plugin';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';

export default [
  {
    ignores: ['dist', 'node_modules', 'coverage'],
  },
  ...tseslint.configs['flat/recommended'],
  {
    files: ['**/*.{ts,tsx}'],
    plugins: {
      ...reactHooks.configs.flat.recommended.plugins,
      'react-refresh': reactRefresh,
    },
    rules: {
      'react-hooks/rules-of-hooks': 'error',
      'react-refresh/only-export-components': 'off',
      'react-hooks/exhaustive-deps': 'off',
      '@typescript-eslint/no-unused-vars': [
        'error',
        {
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          caughtErrorsIgnorePattern: '^_',
        },
      ],
      '@typescript-eslint/no-unsafe-declaration-merging': 'off',
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/ban-ts-comment': [
        'warn',
        {
          'ts-ignore': 'allow-with-description',
          minimumDescriptionLength: 6,
        },
      ],
      'no-restricted-syntax': [
        'warn',
        {
          selector: "CallExpression[callee.object.name='window'][callee.property.name='prompt']",
          message: 'Use the unified dialog/form flow instead of window.prompt.',
        },
        {
          selector: "CallExpression[callee.object.name='window'][callee.property.name='confirm']",
          message: 'Use the unified dialog/form flow instead of window.confirm.',
        },
      ],
    },
  },
];
