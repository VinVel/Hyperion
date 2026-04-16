// @ts-check

import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import tseslint from 'typescript-eslint';
import reactPlugin from 'eslint-plugin-react';
import unusedImports from "eslint-plugin-unused-imports";

export default defineConfig([
  js.configs.recommended,
  ...tseslint.configs.recommended,

  {
    ...reactPlugin.configs.flat.recommended,
    settings: {
      react: {
        version: 'detect',
      },
    },
  },

  reactPlugin.configs.flat['jsx-runtime'], // Add this if you are using React 17+

   {
    plugins: {
      "unused-imports": unusedImports,
    },
    rules: {
      "no-unused-vars": "off", // Disable the default rule
      "unused-imports/no-unused-imports": "error", // Detect unused imports
      "unused-imports/no-unused-vars": [
        "warn",
        {
          vars: "all",
          varsIgnorePattern: "^_",
          args: "after-used",
          argsIgnorePattern: "^_",
        },
      ],
    },
  },
]);
