<script lang="ts">
 /**
  * Phase VI-γ-1: 再起動 & プロファイル切替モーダル。
  *
  * - 現 conf の継続 / 別プロファイルの選択 を 1 つのダイアログで扱う
  * - `api.profiles()` で候補取得、`api.restart()` で発火
  * - 実行後は WS 切断 → 再接続 → `heartbeat` で `confSyncStore.markSynced()` の流れに乗る
  *
  * 親コンポーネントから `open` を bind:open で渡し、true で表示する。
  */
 import { api } from './api';
 import { ControlApiError } from './types';
 import type { ProfileEntry } from './types';
 import { toastStore } from './toasts.svelte';
 import { confSyncStore } from './confSync.svelte';

 interface Props {
  open: boolean;
 }
 let { open = $bindable(false) }: Props = $props();

 let loading = $state(false);
 let submitting = $state(false);
 let entries: ProfileEntry[] = $state([]);
 let currentPath: string | null = $state(null);
 let selectedPath: string | null = $state(null);
 let gracefulMs = $state(800);
 let error: string | null = $state(null);

 async function load() {
  loading = true;
  error = null;
  try {
   const res = await api.profiles();
   entries = res.entries;
   currentPath = res.current;
   const cur = entries.find((e) => e.is_current);
   selectedPath = cur?.path ?? currentPath;
  } catch (e) {
   error = e instanceof Error ? e.message : String(e);
  } finally {
   loading = false;
  }
 }

 $effect(() => {
  if (open) {
   void load();
  }
 });

 function onKeyDown(e: KeyboardEvent) {
  if (!open) return;
  if (e.key === 'Escape' && !submitting) {
   open = false;
  }
 }

 async function submit() {
  submitting = true;
  error = null;
  // selectedPath を現 conf と比較して、同一なら body.conf を省略する。
  const isSame = selectedPath && currentPath && selectedPath === currentPath;
  const body = isSame
   ? { graceful_ms: gracefulMs }
   : { conf: selectedPath ?? undefined, graceful_ms: gracefulMs };
  try {
   const res = await api.restart(body);
   confSyncStore.requestRestart();
   toastStore.info(
    isSame ? 'VAC を再起動します…' : 'プロファイル切替で再起動します…',
    `graceful=${res.graceful_ms}ms new pid=${res.new_pid}`,
   );
   open = false;
  } catch (e) {
   const msg =
    e instanceof ControlApiError
     ? `${e.status} ${e.statusText}: ${JSON.stringify(e.body)}`
     : e instanceof Error
      ? e.message
      : String(e);
   error = msg;
   toastStore.error('再起動に失敗しました', msg);
  } finally {
   submitting = false;
  }
 }

 function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / 1024 / 1024).toFixed(1)} MiB`;
 }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if open}
 <div
  class="fixed inset-0 z-40 flex items-center justify-center bg-black/50 p-4"
  role="dialog"
  aria-modal="true"
  aria-labelledby="restart-dialog-title"
 >
  <div class="w-full max-w-xl rounded-lg border border-surface-200-800 bg-surface-50-950 shadow-xl">
   <header class="flex items-center justify-between border-b border-surface-200-800 px-5 py-3">
    <h2 id="restart-dialog-title" class="text-base font-semibold">
     VAC を再起動 / プロファイル切替
    </h2>
    <button
     type="button"
     class="text-lg leading-none opacity-70 hover:opacity-100"
     onclick={() => (open = false)}
     disabled={submitting}
     aria-label="閉じる"
    >
     ×
    </button>
   </header>

   <div class="space-y-4 px-5 py-4">
    {#if loading}
     <p class="text-sm opacity-70">プロファイル候補を読み込み中…</p>
    {:else if error}
     <p class="rounded border border-error-500/40 bg-error-500/10 p-2 text-xs text-error-900-100">
      {error}
     </p>
    {/if}

    <div>
     <div class="mb-1 text-xs opacity-70">プロファイル（conf ファイル）</div>
     <div class="max-h-64 overflow-y-auto rounded border border-surface-200-800">
      {#if entries.length === 0}
       <p class="p-3 text-xs opacity-60">候補なし。現 conf で再起動します。</p>
      {:else}
       <ul>
        {#each entries as e (e.path)}
         <li>
          <label
           class="flex cursor-pointer items-center gap-3 border-b border-surface-200-800 px-3 py-2 last:border-b-0 hover:bg-surface-100-900"
          >
           <input
            type="radio"
            name="profile"
            value={e.path}
            checked={selectedPath === e.path}
            onchange={() => (selectedPath = e.path)}
           />
           <span class="flex-1 min-w-0">
            <span class="block truncate text-sm font-medium">
             {e.label}
             {#if e.is_current}
              <span class="ml-2 rounded bg-primary-500/20 px-1.5 py-0.5 text-xs text-primary-500">
               current
              </span>
             {/if}
            </span>
            <span class="block truncate text-xs opacity-60">{e.filename} · {fmtBytes(e.size)}</span>
           </span>
          </label>
         </li>
        {/each}
       </ul>
      {/if}
     </div>
     {#if currentPath}
      <p class="mt-1 text-xs opacity-60">current: {currentPath}</p>
     {/if}
    </div>

    <div>
     <label class="block text-xs opacity-70" for="restart-graceful-ms">
      graceful_ms（現プロセスを落とすまでの猶予）
     </label>
     <input
      id="restart-graceful-ms"
      type="number"
      min="0"
      max="10000"
      step="100"
      bind:value={gracefulMs}
      class="mt-1 w-32 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-sm"
     />
    </div>

    <p class="rounded border border-warning-500/40 bg-warning-500/10 p-2 text-xs">
     再起動すると WS が一時切断されます。GUI は自動で再接続しますが、
     進行中の OAuth DCF や音声合成は新プロセスに引き継がれません。
    </p>
   </div>

   <footer class="flex items-center justify-end gap-2 border-t border-surface-200-800 px-5 py-3">
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1.5 text-sm hover:bg-surface-100-900"
     onclick={() => (open = false)}
     disabled={submitting}
    >
     キャンセル
    </button>
    <button
     type="button"
     class="rounded bg-primary-500 px-3 py-1.5 text-sm font-semibold text-white hover:bg-primary-600 disabled:opacity-50"
     onclick={submit}
     disabled={submitting || loading}
    >
     {submitting ? '再起動中…' : '再起動'}
    </button>
   </footer>
  </div>
 </div>
{/if}
