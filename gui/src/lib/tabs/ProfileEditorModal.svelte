<script lang="ts">
 /**
  * Phase VI-γ-4b: プロファイル TOML エディタ（モーダル）。
  *
  * - `<textarea>` で conf.toml の全文を直接編集する素朴な実装。
  * - 保存時にサーバ側で `toml::from_str::<Conf>` が走り、パース不正なら 400 を返して
  *   ファイルは一切書き換わらない（戻ってきた detail を UI に出す）。
  * - `is_current` なファイルを編集した場合はサーバが `warning` を返すので、それをトーストに流す。
  *
  * 将来的な拡張候補:
  *   - Monaco / CodeMirror の TOML シンタックスハイライト
  *   - 差分ビュー（バックアップから diff）
  *   - 自動保存
  */
 import { onMount } from 'svelte';
 import { api } from '../api';
 import { ControlApiError } from '../types';
 import { toastStore } from '../toasts.svelte';

 interface Props {
  filename: string;
  onClose: (didSave: boolean) => void;
 }
 let { filename, onClose }: Props = $props();

 let loading = $state(true);
 let saving = $state(false);
 let originalContent = $state('');
 let content = $state('');
 let isCurrent = $state(false);
 let size = $state(0);
 let error = $state<string | null>(null);

 const dirty = $derived(content !== originalContent);

 onMount(() => {
  void load();
 });

 async function load() {
  loading = true;
  error = null;
  try {
   const res = await api.profileContent(filename);
   originalContent = res.content;
   content = res.content;
   isCurrent = res.is_current;
   size = res.size;
  } catch (e) {
   error = extractMessage(e);
  } finally {
   loading = false;
  }
 }

 function extractMessage(e: unknown): string {
  if (e instanceof ControlApiError) {
   const body = e.body as { detail?: string; error?: string } | null;
   return body?.detail ?? body?.error ?? `${e.status} ${e.statusText}`;
  }
  if (e instanceof Error) return e.message;
  return String(e);
 }

 async function save() {
  saving = true;
  error = null;
  try {
   const res = await api.profilePutContent(filename, { content });
   toastStore.success(
    'プロファイルを保存しました',
    res.warning ?? (res.backup ? `バックアップ: ${res.backup}` : filename),
   );
   if (res.warning) {
    toastStore.warn('再起動が必要です', res.warning);
   }
   originalContent = content;
   onClose(true);
  } catch (e) {
   error = extractMessage(e);
  } finally {
   saving = false;
  }
 }

 function requestClose() {
  if (saving) return;
  if (dirty) {
   const ok = window.confirm('未保存の変更があります。破棄して閉じますか？');
   if (!ok) return;
  }
  onClose(false);
 }

 function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
   requestClose();
  } else if ((e.ctrlKey || e.metaKey) && e.key === 's') {
   e.preventDefault();
   if (!saving && !loading && dirty) {
    void save();
   }
  }
 }
</script>

<svelte:window onkeydown={onKeyDown} />

<div
 class="fixed inset-0 z-40 flex items-center justify-center bg-black/50 p-4"
 role="dialog"
 aria-modal="true"
 aria-labelledby="profile-editor-title"
>
 <div class="flex h-[min(90vh,900px)] w-full max-w-4xl flex-col rounded-lg border border-surface-200-800 bg-surface-50-950 shadow-xl">
  <header class="flex items-center justify-between border-b border-surface-200-800 px-5 py-3">
   <div class="min-w-0">
    <h2 id="profile-editor-title" class="truncate text-base font-semibold">
     編集: {filename}
    </h2>
    <p class="text-xs opacity-60">
     {#if isCurrent}
      <span class="mr-2 rounded bg-warning-500/20 px-1.5 py-0.5 text-warning-500">current</span>
      このファイルは現在読み込まれています。保存しても再起動するまで反映されません。
     {:else}
      {size.toLocaleString()} bytes
     {/if}
    </p>
   </div>
   <button
    type="button"
    class="text-lg leading-none opacity-70 hover:opacity-100"
    onclick={requestClose}
    disabled={saving}
    aria-label="閉じる"
   >
    ×
   </button>
  </header>

  <div class="min-h-0 flex-1 overflow-hidden p-3">
   {#if loading}
    <p class="p-2 text-sm opacity-70">読み込み中…</p>
   {:else}
    <textarea
     bind:value={content}
     class="h-full w-full resize-none rounded border border-surface-200-800 bg-surface-100-900 p-3 font-mono text-xs leading-relaxed focus:outline-none focus:ring-1 focus:ring-primary-500"
     spellcheck="false"
     autocomplete="off"
     autocapitalize="off"
     disabled={saving}
    ></textarea>
   {/if}
  </div>

  {#if error}
   <p class="mx-3 mb-2 rounded border border-error-500/40 bg-error-500/10 p-2 text-xs text-error-900-100">
    {error}
   </p>
  {/if}

  <footer class="flex items-center justify-between border-t border-surface-200-800 px-5 py-3">
   <span class="text-xs opacity-60">
    {#if dirty}
     未保存の変更があります
    {:else}
     変更なし
    {/if}
   </span>
   <div class="flex items-center gap-2">
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1.5 text-sm hover:bg-surface-100-900"
     onclick={requestClose}
     disabled={saving}
    >
     閉じる
    </button>
    <button
     type="button"
     class="rounded bg-primary-500 px-3 py-1.5 text-sm font-semibold text-white hover:bg-primary-600 disabled:opacity-50"
     onclick={() => void save()}
     disabled={saving || loading || !dirty}
    >
     {saving ? '保存中…' : '保存'}
    </button>
   </div>
  </footer>
 </div>
</div>
