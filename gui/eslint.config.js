// ESLint flat config (ESLint v9+)。
//
// 目的:
//   1. 一般的な JS/TS の潜在バグと unused symbol を拾う
//   2. **Svelte 5 runes を `.ts` / `.js` ファイルで使用するのを禁止**（β-2 で踏んだ罠）
//      runes は vite-plugin-svelte が変換するので、`.svelte` または `.svelte.ts` /
//      `.svelte.js` 以外で書くと runtime で `rune_outside_svelte` エラーになる。
//      svelte-check も tsc もビルドも通るので、**Lint でしか検出できない**
//   3. Svelte テンプレート内の valid-compile エラーを CI 相当に可視化
//
// 運用:
//   - `npm run lint` で走らせる
//   - エディタは ESLint 拡張を入れればリアルタイム通知
//   - Prettier は今のところ使っていないため連携なし

import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import svelteParser from 'svelte-eslint-parser';

export default tseslint.config(
 // 除外
 {
  ignores: ['dist/**', 'node_modules/**', '.svelte-kit/**'],
 },

 // 1) ベース: JS + TS recommended
 js.configs.recommended,
 ...tseslint.configs.recommended,

 // 2) Svelte 5 用
 ...svelte.configs['flat/recommended'],

 // 3) .svelte ファイルは svelte-eslint-parser を通し、<script lang="ts"> は TS parser にデリゲート
 {
  files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
  languageOptions: {
   parser: svelteParser,
   parserOptions: {
    parser: tseslint.parser,
    extraFileExtensions: ['.svelte'],
   },
   globals: {
    ...globals.browser,
   },
  },
 },

 // 4) 通常の TS/JS ファイル
 {
  files: ['**/*.ts', '**/*.js'],
  languageOptions: {
   globals: {
    ...globals.browser,
    ...globals.node,
   },
  },
 },

 // 5) **最重要**: .ts / .js ファイル内で Svelte 5 runes を禁止
 //    `.svelte.ts` / `.svelte.js` / `.svelte` には適用しない
 {
  files: ['**/*.ts', '**/*.js'],
  ignores: ['**/*.svelte.ts', '**/*.svelte.js', '**/*.svelte'],
  rules: {
   'no-restricted-syntax': [
    'error',
    {
     selector:
      "CallExpression[callee.name=/^\\$(state|derived|effect|props|bindable|inspect|host)$/]",
     message:
      'Svelte 5 runes ($state/$derived/$effect/…) can only be used in .svelte or .svelte.ts/.svelte.js files. Rename this file or move the rune to a Svelte module.',
    },
    {
     selector:
      "MemberExpression[object.name=/^\\$(derived|state|effect|inspect)$/]",
     message:
      'Svelte 5 rune members (e.g. $derived.by / $state.raw) can only be used in .svelte or .svelte.ts/.svelte.js files.',
    },
   ],
  },
 },

 // 6) プロジェクト固有の調整
 {
  rules: {
   // unused import は error、`_` プレフィックス付きは許容
   '@typescript-eslint/no-unused-vars': [
    'error',
    {
     argsIgnorePattern: '^_',
     varsIgnorePattern: '^_',
     caughtErrorsIgnorePattern: '^_',
    },
   ],
   // Control API の I/O 境界で `any` を意図的に使う箇所があるので warn に下げる
   '@typescript-eslint/no-explicit-any': 'warn',
  },
 },

 // 7) 設定ファイル自身
 {
  files: ['eslint.config.js', 'vite.config.ts', 'svelte.config.js'],
  languageOptions: {
   globals: {
    ...globals.node,
   },
  },
 },
);
