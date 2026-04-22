<script lang="ts">
 /**
  * Phase VI-γ-4b: プロファイル（conf ファイル）管理パネル。
  *
  * - `api.profiles()` で一覧を取得し、複製・リネーム・削除・編集・切替起動を GUI から実行できる。
  * - 編集は本ファイル内で `ProfileEditorModal` を開いて `<textarea>` で TOML を直接いじる。
  *   より高機能なエディタ（Monaco 等）への差し替えは後続フェーズで。
  * - 現 conf（`is_current`）のリネーム・削除は disable。編集は「再起動するまで反映されない」旨を警告する。
  */
 import { api } from '../api';
 import { ControlApiError } from '../types';
 import type { ProfileEntry } from '../types';
 import { toastStore } from '../toasts.svelte';
 import ProfileEditorModal from './ProfileEditorModal.svelte';

 let loading = $state(false);
 let entries = $state<ProfileEntry[]>([]);
 let currentPath = $state<string | null>(null);
 let directory = $state<string | null>(null);
 let error = $state<string | null>(null);

 // インライン入力系の UI 状態: 「複製中の行」「リネーム中の行」を filename で持つ。
 let cloneFor = $state<string | null>(null);
 let cloneName = $state('');
 let renameFor = $state<string | null>(null);
 let renameName = $state('');
 let busyName = $state<string | null>(null);

 // 編集モーダル
 let editingFilename = $state<string | null>(null);

 async function load() {
  loading = true;
  error = null;
  try {
   const res = await api.profiles();
   entries = res.entries;
   currentPath = res.current;
   directory = res.directory;
  } catch (e) {
   error = extractMessage(e);
  } finally {
   loading = false;
  }
 }

 $effect(() => {
  void load();
 });

 function extractMessage(e: unknown): string {
  if (e instanceof ControlApiError) {
   const body = e.body as { detail?: string; error?: string } | null;
   return body?.detail ?? body?.error ?? `${e.status} ${e.statusText}`;
  }
  if (e instanceof Error) return e.message;
  return String(e);
 }

 function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / 1024 / 1024).toFixed(1)} MiB`;
 }

 function fmtModified(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.valueOf())) return iso;
  return d.toLocaleString();
 }

 function startClone(entry: ProfileEntry) {
  cloneFor = entry.filename;
  renameFor = null;
  // `foo.toml` → `foo.copy.toml` のような初期値
  const base = entry.filename.replace(/\.toml$/i, '');
  cloneName = `${base}.copy.toml`;
 }

 function cancelClone() {
  cloneFor = null;
  cloneName = '';
 }

 async function submitClone(source: string) {
  const trimmed = cloneName.trim();
  if (!trimmed) return;
  busyName = source;
  try {
   const res = await api.profileClone({ source, new_filename: trimmed });
   toastStore.success('プロファイルを複製しました', res.filename);
   cloneFor = null;
   cloneName = '';
   await load();
  } catch (e) {
   toastStore.error('複製に失敗しました', extractMessage(e));
  } finally {
   busyName = null;
  }
 }

 function startRename(entry: ProfileEntry) {
  if (entry.is_current) return;
  renameFor = entry.filename;
  cloneFor = null;
  renameName = entry.filename;
 }

 function cancelRename() {
  renameFor = null;
  renameName = '';
 }

 async function submitRename(original: string) {
  const trimmed = renameName.trim();
  if (!trimmed || trimmed === original) {
   cancelRename();
   return;
  }
  busyName = original;
  try {
   const res = await api.profileRename(original, { new_filename: trimmed });
   toastStore.success('プロファイルをリネームしました', `${original} → ${res.filename}`);
   renameFor = null;
   renameName = '';
   await load();
  } catch (e) {
   toastStore.error('リネームに失敗しました', extractMessage(e));
  } finally {
   busyName = null;
  }
 }

 async function doDelete(entry: ProfileEntry) {
  if (entry.is_current) return;
  const ok = window.confirm(
   `プロファイル "${entry.filename}" を削除しますか？\n` +
    `（実体はバックアップファイル .bak-... に退避されます）`,
  );
  if (!ok) return;
  busyName = entry.filename;
  try {
   const res = await api.profileDelete(entry.filename);
   toastStore.success('プロファイルを削除しました', `バックアップ: ${res.backup ?? '(作成されませんでした)'}`);
   await load();
  } catch (e) {
   toastStore.error('削除に失敗しました', extractMessage(e));
  } finally {
   busyName = null;
  }
 }

 function openEditor(entry: ProfileEntry) {
  editingFilename = entry.filename;
 }
 function onEditorClosed(didSave: boolean) {
  editingFilename = null;
  if (didSave) {
   void load();
  }
 }

 async function switchTo(entry: ProfileEntry) {
  if (entry.is_current) return;
  const ok = window.confirm(
   `プロファイル "${entry.filename}" に切り替えて再起動しますか？\n` +
    `WebSocket が一時的に切断されます。`,
  );
  if (!ok) return;
  busyName = entry.filename;
  try {
   const res = await api.restart({ conf: entry.path, graceful_ms: 800 });
   toastStore.info('再起動します…', `new pid=${res.new_pid}`);
  } catch (e) {
   toastStore.error('再起動に失敗しました', extractMessage(e));
  } finally {
   busyName = null;
  }
 }
</script>

<section class="rounded border border-surface-200-800 bg-surface-50-950">
 <header class="flex items-center justify-between border-b border-surface-200-800 px-4 py-2">
  <div>
   <h3 class="text-sm font-semibold">プロファイル管理</h3>
   <p class="text-xs opacity-60">
    {directory ?? '読み込み中…'}
   </p>
  </div>
  <div class="flex items-center gap-2">
   <button
    type="button"
    class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-100-900"
    onclick={() => void load()}
    disabled={loading}
   >
    {loading ? '読込中…' : '更新'}
   </button>
  </div>
 </header>

 {#if error}
  <p class="border-b border-error-500/40 bg-error-500/10 px-4 py-2 text-xs text-error-900-100">
   {error}
  </p>
 {/if}

 {#if entries.length === 0 && !loading}
  <p class="p-4 text-xs opacity-60">プロファイル候補なし。</p>
 {:else}
  <ul class="divide-y divide-surface-200-800">
   {#each entries as entry (entry.path)}
    {@const isBusy = busyName === entry.filename}
    <li class="px-4 py-3 text-sm">
     <div class="flex flex-wrap items-start gap-2">
      <div class="min-w-0 flex-1">
       <div class="flex items-center gap-2">
        <span class="truncate font-medium">{entry.label}</span>
        {#if entry.is_current}
         <span class="rounded bg-primary-500/20 px-1.5 py-0.5 text-xs text-primary-500">current</span>
        {/if}
       </div>
       <div class="truncate text-xs opacity-60">
        {entry.filename} · {fmtBytes(entry.size)} · {fmtModified(entry.modified)}
       </div>
      </div>
      <div class="flex flex-wrap items-center gap-1">
       <button
        type="button"
        class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900 disabled:opacity-50"
        onclick={() => openEditor(entry)}
        disabled={isBusy}
       >
        編集
       </button>
       <button
        type="button"
        class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900 disabled:opacity-50"
        onclick={() => startClone(entry)}
        disabled={isBusy}
       >
        複製
       </button>
       <button
        type="button"
        class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900 disabled:opacity-50"
        onclick={() => startRename(entry)}
        disabled={isBusy || entry.is_current}
        title={entry.is_current ? '現在読み込み中のプロファイルはリネームできません' : ''}
       >
        リネーム
       </button>
       <button
        type="button"
        class="rounded border border-error-500/40 px-2 py-0.5 text-xs text-error-500 hover:bg-error-500/10 disabled:opacity-50"
        onclick={() => void doDelete(entry)}
        disabled={isBusy || entry.is_current}
        title={entry.is_current ? '現在読み込み中のプロファイルは削除できません' : ''}
       >
        削除
       </button>
       <button
        type="button"
        class="rounded bg-primary-500 px-2 py-0.5 text-xs font-medium text-white hover:bg-primary-600 disabled:opacity-50"
        onclick={() => void switchTo(entry)}
        disabled={isBusy || entry.is_current}
       >
        切替起動
       </button>
      </div>
     </div>

     {#if cloneFor === entry.filename}
      <div class="mt-2 flex flex-wrap items-center gap-2 rounded border border-surface-300-700 bg-surface-100-900 p-2">
       <span class="text-xs opacity-70">複製先ファイル名:</span>
       <input
        type="text"
        bind:value={cloneName}
        class="flex-1 min-w-0 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
        placeholder="conf.backup.toml"
       />
       <button
        type="button"
        class="rounded bg-primary-500 px-2 py-0.5 text-xs font-medium text-white hover:bg-primary-600 disabled:opacity-50"
        onclick={() => void submitClone(entry.filename)}
        disabled={isBusy || !cloneName.trim()}
       >
        実行
       </button>
       <button
        type="button"
        class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-50-950"
        onclick={cancelClone}
        disabled={isBusy}
       >
        取消
       </button>
      </div>
     {/if}

     {#if renameFor === entry.filename}
      <div class="mt-2 flex flex-wrap items-center gap-2 rounded border border-surface-300-700 bg-surface-100-900 p-2">
       <span class="text-xs opacity-70">新しいファイル名:</span>
       <input
        type="text"
        bind:value={renameName}
        class="flex-1 min-w-0 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
       />
       <button
        type="button"
        class="rounded bg-primary-500 px-2 py-0.5 text-xs font-medium text-white hover:bg-primary-600 disabled:opacity-50"
        onclick={() => void submitRename(entry.filename)}
        disabled={isBusy || !renameName.trim()}
       >
        実行
       </button>
       <button
        type="button"
        class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-50-950"
        onclick={cancelRename}
        disabled={isBusy}
       >
        取消
       </button>
      </div>
     {/if}
    </li>
   {/each}
  </ul>
 {/if}

 {#if currentPath}
  <footer class="border-t border-surface-200-800 px-4 py-2 text-xs opacity-60">
   current: {currentPath}
  </footer>
 {/if}
</section>

{#if editingFilename}
 <ProfileEditorModal filename={editingFilename} onClose={onEditorClosed} />
{/if}
