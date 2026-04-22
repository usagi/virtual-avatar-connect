import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import tailwindcss from '@tailwindcss/vite';

// ---------------------------------------------------------------------------
// Phase VI-β: VAC GUI Vite 設定
//
// - `base: '/gui/'` にして、本番は VAC 本体の Actix-web (`/gui/*`) で静的配信する。
// - dev サーバ (`npm run dev`) は 5173 を使い、Control API (`/api/v1/control`)
//   と既存 WS (`/websocket`) を VAC (127.0.0.1:57000) に HTTP/WS proxy する。
//   これで GUI 開発中も VAC を再起動せず、ホットリロードだけで動作を追える。
// - Tailwind v4 は plugin として組み込み、設定ファイルは廃止（v4 式）。
// ---------------------------------------------------------------------------

const VAC_BACKEND = process.env.VAC_BACKEND_URL ?? 'http://127.0.0.1:57000';

export default defineConfig({
 base: '/gui/',
 plugins: [svelte(), tailwindcss()],
 server: {
  port: 5173,
  strictPort: true,
  proxy: {
   '/api/v1/control': {
    target: VAC_BACKEND,
    changeOrigin: false,
    ws: true,
   },
   '/websocket': {
    target: VAC_BACKEND,
    changeOrigin: false,
    ws: true,
   },
   '/browser-output': {
    target: VAC_BACKEND,
    changeOrigin: false,
   },
   '/resources': {
    target: VAC_BACKEND,
    changeOrigin: false,
   },
  },
 },
 build: {
  outDir: 'dist',
  emptyOutDir: true,
  sourcemap: true,
 },
});
