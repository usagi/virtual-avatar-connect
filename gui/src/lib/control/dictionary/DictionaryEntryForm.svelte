<script lang="ts">
 /**
  * Phase φ-3c: 辞書エントリ新規作成 / 編集ダイアログ。
  *
  * - `mode == 'new'` で POST `/table/{key}/entry`、`mode == 'edit'` で PATCH `/table/{key}/entry/{row_index}`。
  * - すべて `If-Match: b3:<content_hash>` を付与して楽観ロックを効かせる。
  * - 409 Conflict を受けたら `on_conflict` callback にサーバ応答を渡す（親で ConflictDialog を出す）。
  * - フォーム内 validation は最小限（source / replacement 必須、priority は整数）に留め、
  *   サーバ側 422 パース失敗も message でフォールバック表示できるようにする。
  * - 11 カラムすべてのうち `created_at` / `by` は new モードで自動補完、edit モードでは read-only。
  *
  * 親から bind:open / open=false で dismiss する標準パターン。
  */

 import { api } from '../../api';
 import { ControlApiError } from '../../types';
 import type {
  TableEntryDto,
  TableEntryRequest,
  TableFileDto,
  TableMutationResponse
 } from '../../types';
 import { toastStore } from '../../toasts.svelte';

 export type EntryFormMode =
  | { kind: 'new' }
  | { kind: 'edit'; row: TableEntryDto };

 /** 409 時に親へ渡すコンテキスト。親は mode / myValues / base（edit 時）から
  * 3-way 衝突解決 UI を組み立てる（φ-3d）。 */
 export type EntryFormConflictContext = {
  body: unknown;
  mode: EntryFormMode;
  myValues: Record<string, unknown>;
  /** 送信時点の table.content_hash（親の refetch 後ハッシュ差分をデバッグ表示する用）。 */
  priorHash: string;
 };

 type Props = {
  open: boolean;
  table: TableFileDto;
  mode: EntryFormMode;
  /** 成功時に親で store を差し替える用。サーバからは mutation response しか返ってこないため、
   * 親側は response の content_hash を保持した上で Table 全体を再読込する設計にする。 */
  onSaved?: (res: TableMutationResponse) => void;
  /** 409 Conflict 時に呼ぶ（φ-3d の ConflictDialog を親から開くためのフック）。 */
  onConflict?: (ctx: EntryFormConflictContext) => void;
 };

 let { open = $bindable(false), table, mode, onSaved, onConflict }: Props = $props();

 // --- フォーム state -------------------------------------------------------

 let sourceStr = $state('');
 let replacementStr = $state('');
 let kind = $state<'literal' | 'regex'>('literal');
 let priority = $state(0);
 let enabled = $state(true);
 let isLocked = $state(false);
 let byStr = $state('');
 let createdAt = $state('');
 let expiresAt = $state('');
 let tagsStr = $state('');
 let note = $state('');

 let submitting = $state(false);
 let formError = $state<string | null>(null);

 // --- mode に応じた初期値プレ埋め ----------------------------------------

 function primeFromMode(): void {
  if (mode.kind === 'new') {
   sourceStr = '';
   replacementStr = '';
   kind = 'literal';
   priority = 0;
   enabled = true;
   isLocked = false;
   byStr = 'gui:user';
   createdAt = new Date().toISOString();
   expiresAt = '';
   tagsStr = '';
   note = '';
  } else {
   const v = mode.row.values;
   sourceStr = str(v.source);
   replacementStr = str(v.replacement);
   const k = str(v.kind);
   kind = k === 'regex' ? 'regex' : 'literal';
   priority = num(v.priority, 0);
   enabled = bool(v.enabled, true);
   isLocked = bool(v.is_locked, false);
   byStr = str(v.by);
   createdAt = str(v.created_at);
   expiresAt = str(v.expires_at);
   tagsStr = str(v.tags);
   note = str(v.note);
  }
  formError = null;
 }

 $effect(() => {
  if (open) primeFromMode();
 });

 function str(v: unknown): string {
  if (v == null) return '';
  return typeof v === 'string' ? v : String(v);
 }
 function num(v: unknown, def: number): number {
  if (typeof v === 'number') return v;
  if (typeof v === 'string') {
   const n = Number(v);
   return Number.isFinite(n) ? n : def;
  }
  return def;
 }
 function bool(v: unknown, def: boolean): boolean {
  if (typeof v === 'boolean') return v;
  if (typeof v === 'string') return v === 'true' || v === '1';
  if (typeof v === 'number') return v !== 0;
  return def;
 }

 // --- submit --------------------------------------------------------------

 /**
  * 11 カラム schema の列だけ filter した map を組み立て、未記入の値は null にして送る。
  * サーバ側 `map_to_row` が未指定列を null で埋めるため、ここで厳密に全列埋める必要は無い。
  */
 function buildValues(): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  out.source = sourceStr;
  out.replacement = replacementStr;
  out.kind = kind;
  out.priority = priority;
  out.enabled = enabled;
  out.is_locked = isLocked;
  if (byStr) out.by = byStr;
  if (createdAt) out.created_at = createdAt;
  out.expires_at = expiresAt || null;
  out.tags = tagsStr;
  out.note = note;
  return out;
 }

 async function submit(): Promise<void> {
  if (submitting) return;
  if (!sourceStr.trim()) {
   formError = 'source は必須です';
   return;
  }
  if (!replacementStr.trim()) {
   formError = 'replacement は必須です';
   return;
  }
  if (!Number.isInteger(priority)) {
   formError = 'priority は整数値にしてください';
   return;
  }
  submitting = true;
  formError = null;
  const values = buildValues();
  const body: TableEntryRequest = { values };
  const priorHash = table.content_hash;
  try {
   let res: TableMutationResponse;
   if (mode.kind === 'new') {
    res = await api.postControlTableEntry(table.key, body, priorHash);
    toastStore.success('辞書に追加しました', `row_index=${res.affected_row_index ?? '?'}`);
   } else {
    res = await api.patchControlTableEntry(
     table.key,
     mode.row.row_index,
     body,
     priorHash
    );
    toastStore.success('辞書を更新しました', `row_index=${mode.row.row_index}`);
   }
   onSaved?.(res);
   open = false;
  } catch (e) {
   if (e instanceof ControlApiError && e.status === 409) {
    onConflict?.({ body: e.body, mode, myValues: values, priorHash });
    open = false;
    return;
   }
   formError = extractMessage(e);
  } finally {
   submitting = false;
  }
 }

 function extractMessage(e: unknown): string {
  if (e instanceof ControlApiError) {
   const body = e.body as { detail?: string; error?: string; reason?: string } | null;
   return body?.detail ?? body?.reason ?? body?.error ?? `${e.status} ${e.statusText}`;
  }
  if (e instanceof Error) return e.message;
  return String(e);
 }

 function onKeyDown(e: KeyboardEvent): void {
  if (!open) return;
  if (e.key === 'Escape' && !submitting) {
   open = false;
  }
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey) && !submitting) {
   void submit();
  }
 }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if open}
 <div
  class="fixed inset-0 z-40 flex items-center justify-center bg-black/50 p-4"
  role="dialog"
  aria-modal="true"
  aria-labelledby="dict-entry-form-title"
 >
  <div class="w-full max-w-2xl rounded-lg border border-surface-200-800 bg-surface-50-950 shadow-xl">
   <header class="flex items-center justify-between border-b border-surface-200-800 px-5 py-3">
    <h2 id="dict-entry-form-title" class="text-sm font-semibold">
     {mode.kind === 'new' ? '辞書エントリを追加' : `辞書エントリを編集 (row_index=${mode.row.row_index})`}
    </h2>
    <button
     type="button"
     class="text-lg leading-none opacity-70 hover:opacity-100"
     onclick={() => (open = false)}
     disabled={submitting}
     aria-label="閉じる">×</button>
   </header>

   <div class="grid max-h-[70vh] grid-cols-1 gap-3 overflow-y-auto px-5 py-4 text-xs md:grid-cols-2">
    <label class="md:col-span-2 flex flex-col gap-1">
     <span class="opacity-70">source<span class="text-error-500"> *</span></span>
     <input
      type="text"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
      bind:value={sourceStr}
      placeholder="にんげん"
      autocomplete="off" />
    </label>

    <label class="md:col-span-2 flex flex-col gap-1">
     <span class="opacity-70">replacement<span class="text-error-500"> *</span></span>
     <input
      type="text"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
      bind:value={replacementStr}
      placeholder="人間" />
    </label>

    <label class="flex flex-col gap-1">
     <span class="opacity-70">kind</span>
     <select
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
      bind:value={kind}>
      <option value="literal">literal</option>
      <option value="regex">regex</option>
     </select>
    </label>

    <label class="flex flex-col gap-1">
     <span class="opacity-70">priority</span>
     <input
      type="number"
      step="1"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
      bind:value={priority} />
    </label>

    <label class="flex items-center gap-2">
     <input type="checkbox" bind:checked={enabled} />
     <span>enabled</span>
    </label>

    <label class="flex items-center gap-2">
     <input type="checkbox" bind:checked={isLocked} />
     <span>is_locked（保護）</span>
    </label>

    <label class="flex flex-col gap-1">
     <span class="opacity-70">by</span>
     <input
      type="text"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
      bind:value={byStr}
      placeholder="gui:user" />
    </label>

    <label class="flex flex-col gap-1">
     <span class="opacity-70">created_at</span>
     <input
      type="text"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
      bind:value={createdAt}
      readonly={mode.kind === 'edit'} />
    </label>

    <label class="md:col-span-2 flex flex-col gap-1">
     <span class="opacity-70">expires_at（ISO 8601、空欄で無期限）</span>
     <input
      type="text"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
      bind:value={expiresAt}
      placeholder="2026-12-31T00:00:00Z" />
    </label>

    <label class="md:col-span-2 flex flex-col gap-1">
     <span class="opacity-70">tags（カンマ区切り）</span>
     <input
      type="text"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
      bind:value={tagsStr}
      placeholder="arknights,seed" />
    </label>

    <label class="md:col-span-2 flex flex-col gap-1">
     <span class="opacity-70">note</span>
     <textarea
      rows="2"
      class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
      bind:value={note}></textarea>
    </label>

    {#if formError}
     <div class="md:col-span-2 rounded border border-error-500/40 bg-error-500/10 p-2 text-xs text-error-900-100">
      {formError}
     </div>
    {/if}

    <div class="md:col-span-2 text-[0.65rem] opacity-50">
     送信時に <code>If-Match: b3:{table.content_hash.slice(0, 12)}…</code> を付与して楽観ロックします。
     他クライアントで先に更新されていた場合は 409 になり、自動で衝突ダイアログが開きます（φ-3d で実装）。
     Ctrl+Enter で送信、Esc で閉じる。
    </div>
   </div>

   <footer class="flex items-center justify-end gap-2 border-t border-surface-200-800 px-5 py-3">
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
     onclick={() => (open = false)}
     disabled={submitting}>キャンセル</button>
    <button
     type="button"
     class="rounded bg-primary-500 px-3 py-1 text-xs text-white hover:bg-primary-600 disabled:opacity-60"
     onclick={() => void submit()}
     disabled={submitting}>
     {submitting ? '送信中…' : mode.kind === 'new' ? '追加する' : '更新する'}
    </button>
   </footer>
  </div>
 </div>
{/if}
