<script lang="ts">
 /**
  * Phase φ-3d: 楽観ロック衝突（409）時の 3-way 解決ダイアログ。
  *
  * Form.svelte が PATCH で 409 を受けると、親の Pane が:
  *   1. `reloadCurrent()` でサーバ現状を取り直す
  *   2. 自分が編集していた row を server 側で同じ row_index / 同じ source で探す
  *   3. 見つかったら (base=編集開始時の row, mine=フォーム送信値, server=サーバ現状) を
  *      このダイアログに流し込む
  * を行う。ダイアログは 11 カラムを 3 列で並べ、
  *   - 「自分の編集を強制」（全フィールド mine）
  *   - 「サーバ現行を採用」（何もしない、そのまま閉じる）
  *   - 「フィールドごとに merge」（各列で base/mine/server から選択、apply）
  * の 3 パスを提供する。
  *
  * 実 API は `api.patchControlTableEntry(key, row_index, {values}, ifMatch=server.content_hash)`
  * を呼び、If-Match は **server 再取得後のハッシュ** を使うため、さらに衝突することは
  * 実質無い（race で起きても再 open で対応）。
  *
  * new モード（POST）の 409 や delete の 409 は、このダイアログの対象外。
  * 親は toast + reload で対応する（φ-3c で実装済み）。
  */

 import { api } from '../../api';
 import { ControlApiError } from '../../types';
 import type { TableEntryDto, TableFileDto, TableMutationResponse } from '../../types';
 import { toastStore } from '../../toasts.svelte';

 export type ConflictResolution = {
  /** 実 PATCH に使った values。ダイアログ側で組み立てたもの。 */
  values: Record<string, unknown>;
  /** サーバ返却の mutation response（次 If-Match の種）。 */
  response: TableMutationResponse;
  /** 採用戦略（ログ / debug 用）。 */
  strategy: 'mine' | 'server' | 'merge';
 };

 type Side = 'base' | 'mine' | 'server';

 type Props = {
  open: boolean;
  /** 最新 server 状態（409 を受けて再 fetch したもの）。If-Match の種も兼ねる。 */
  serverTable: TableFileDto;
  /** 衝突対象の行を server 側で特定した結果。見つからなければ `null`
   * （= server 側で削除された。ダイアログは限定機能モードに落ちる）。 */
  serverRow: TableEntryDto | null;
  /** Form を開いた時点の値（diff の base）。 */
  baseRow: TableEntryDto;
  /** Form で送信しようとした values。 */
  myValues: Record<string, unknown>;
  onResolved?: (r: ConflictResolution) => void;
  onCancelled?: () => void;
 };

 let {
  open = $bindable(false),
  serverTable,
  serverRow,
  baseRow,
  myValues,
  onResolved,
  onCancelled
 }: Props = $props();

 let submitting = $state(false);
 let formError = $state<string | null>(null);

 // フィールドごとの選択（manual merge モード用）。column name → Side。
 let perField = $state<Record<string, Side>>({});

 // 表示モード: 'summary' は 3 ボタンの最初の画面。'merge' はフィールド選択画面。
 let phase = $state<'summary' | 'merge'>('summary');

 const columns = $derived(serverTable.columns);

 function toStr(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'string') return v;
  return JSON.stringify(v);
 }

 function valueOf(side: Side, col: string): unknown {
  if (side === 'base') return baseRow.values[col];
  if (side === 'mine') return myValues[col];
  return serverRow?.values[col];
 }

 function displayOf(side: Side, col: string): string {
  if (side === 'server' && !serverRow) return '(削除された)';
  return toStr(valueOf(side, col));
 }

 function hasDiff(col: string): { mine: boolean; server: boolean } {
  const b = toStr(baseRow.values[col] ?? null);
  return {
   mine: toStr(myValues[col] ?? null) !== b,
   server: !serverRow ? false : toStr(serverRow.values[col] ?? null) !== b
  };
 }

 const diffCols = $derived(columns.filter((c) => {
  const d = hasDiff(c);
  return d.mine || d.server;
 }));

 // 初期値: mine で開始（サーバ現行と違うフィールドだけ review 対象にしたい直感）。
 function resetMerge(): void {
  const next: Record<string, Side> = {};
  for (const c of columns) next[c] = 'mine';
  perField = next;
 }

 $effect(() => {
  if (open) {
   phase = 'summary';
   formError = null;
   resetMerge();
  }
 });

 function goMergeMode(): void {
  phase = 'merge';
 }

 function buildValuesFromPerField(): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const c of columns) {
   const side = perField[c] ?? 'mine';
   out[c] = valueOf(side, c) ?? null;
  }
  return out;
 }

 async function submitMine(): Promise<void> {
  await doPatch('mine', myValues);
 }

 function acceptServer(): void {
  // 何もしないで閉じる。親は reloadCurrent() 済みなので画面は server 最新。
  toastStore.info('サーバ現行を採用しました', '自分の編集は破棄されました');
  onCancelled?.();
  open = false;
 }

 async function submitMerge(): Promise<void> {
  const vals = buildValuesFromPerField();
  await doPatch('merge', vals);
 }

 async function doPatch(
  strategy: 'mine' | 'merge',
  values: Record<string, unknown>
 ): Promise<void> {
  if (submitting) return;
  if (!serverRow) {
   formError =
    'サーバ側で対象行が削除されているため PATCH できません。「サーバ現行を採用」で閉じてください。';
   return;
  }
  submitting = true;
  formError = null;
  try {
   const res = await api.patchControlTableEntry(
    serverTable.key,
    serverRow.row_index,
    { values },
    serverTable.content_hash
   );
   onResolved?.({ values, response: res, strategy });
   toastStore.success(
    strategy === 'mine' ? '自分の編集を強制反映しました' : 'merge 結果を保存しました'
   );
   open = false;
  } catch (e) {
   if (e instanceof ControlApiError) {
    const body = e.body as { detail?: string; error?: string } | null;
    formError =
     `${e.status} ${e.statusText}: ${body?.detail ?? body?.error ?? ''}` +
     (e.status === 409 ? '（また新しい衝突が発生しました。再読込してやり直してください）' : '');
   } else if (e instanceof Error) {
    formError = e.message;
   } else {
    formError = String(e);
   }
  } finally {
   submitting = false;
  }
 }

 function onKeyDown(ev: KeyboardEvent): void {
  if (!open) return;
  if (ev.key === 'Escape' && !submitting) {
   onCancelled?.();
   open = false;
  }
 }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if open}
 <div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
  role="dialog"
  aria-modal="true"
  aria-labelledby="dict-conflict-title"
 >
  <div class="w-full max-w-4xl rounded-lg border border-warning-500/50 bg-surface-50-950 shadow-xl">
   <header class="flex items-center justify-between border-b border-surface-200-800 px-5 py-3">
    <h2 id="dict-conflict-title" class="text-sm font-semibold text-warning-700-300">
     ⚠ 辞書編集で競合が発生しました (row_index={baseRow.row_index})
    </h2>
    <button
     type="button"
     class="text-lg leading-none opacity-70 hover:opacity-100"
     onclick={() => {
      onCancelled?.();
      open = false;
     }}
     disabled={submitting}
     aria-label="閉じる">×</button>
   </header>

   <div class="max-h-[70vh] overflow-y-auto px-5 py-4 text-xs">
    {#if !serverRow}
     <div class="mb-3 rounded border border-error-500/40 bg-error-500/10 p-2 text-error-900-100">
      他のクライアントがこの行を<strong>削除</strong>しています。<br />
      PATCH は実行できません。「サーバ現行を採用（閉じる）」を押して再検討するか、
      「新規として追加」し直してください。
     </div>
    {/if}

    <p class="mb-3 opacity-80">
     あなたが編集を開始した時点の内容（base）・あなたが保存しようとした内容（mine）・
     現在のサーバ上の内容（server）の 3-way 差分です。
    </p>

    {#if diffCols.length === 0}
     <p class="opacity-70">
      実際に差が出ているカラムはありません（空振り衝突）。サーバ現行を採用して閉じて問題ありません。
     </p>
    {:else}
     <div class="overflow-x-auto rounded border border-surface-300-700">
      <table class="w-full min-w-[40rem] border-collapse text-left text-xs">
       <thead class="bg-surface-100-900 text-[0.65rem] uppercase opacity-80">
        <tr>
         <th class="px-2 py-1">column</th>
         <th class="px-2 py-1">base（編集開始時）</th>
         <th class="px-2 py-1 text-primary-700-300">mine（あなた）</th>
         <th class="px-2 py-1 text-warning-700-300">server（現在）</th>
         {#if phase === 'merge'}
          <th class="px-2 py-1 text-center">採用</th>
         {/if}
        </tr>
       </thead>
       <tbody>
        {#each diffCols as col (col)}
         {@const d = hasDiff(col)}
         <tr class="border-t border-surface-200-800 align-top">
          <td class="px-2 py-1 font-mono">{col}</td>
          <td class="px-2 py-1">
           <pre class="whitespace-pre-wrap break-words opacity-70">{displayOf('base', col)}</pre>
          </td>
          <td class="px-2 py-1" class:bg-primary-500-500={false} class:font-semibold={d.mine}>
           <pre class="whitespace-pre-wrap break-words">{displayOf('mine', col)}</pre>
          </td>
          <td class="px-2 py-1" class:font-semibold={d.server}>
           <pre class="whitespace-pre-wrap break-words">{displayOf('server', col)}</pre>
          </td>
          {#if phase === 'merge'}
           <td class="px-2 py-1 text-center">
            <div class="inline-flex gap-2 text-[0.7rem]">
             <label class="inline-flex items-center gap-1">
              <input type="radio" bind:group={perField[col]} value="base" />
              base
             </label>
             <label class="inline-flex items-center gap-1">
              <input type="radio" bind:group={perField[col]} value="mine" />
              mine
             </label>
             <label class="inline-flex items-center gap-1" class:opacity-50={!serverRow}>
              <input
               type="radio"
               bind:group={perField[col]}
               value="server"
               disabled={!serverRow} />
              server
             </label>
            </div>
           </td>
          {/if}
         </tr>
        {/each}
       </tbody>
      </table>
     </div>
    {/if}

    {#if formError}
     <div class="mt-3 rounded border border-error-500/40 bg-error-500/10 p-2 text-error-900-100">
      {formError}
     </div>
    {/if}

    <div class="mt-3 text-[0.65rem] opacity-50">
     書き戻し時は最新 server hash
     <code>b3:{serverTable.content_hash.slice(0, 12)}…</code>
     を If-Match に使います。
    </div>
   </div>

   <footer class="flex flex-wrap items-center justify-end gap-2 border-t border-surface-200-800 px-5 py-3">
    {#if phase === 'summary'}
     <button
      type="button"
      class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
      onclick={acceptServer}
      disabled={submitting}>サーバ現行を採用（閉じる）</button>
     <button
      type="button"
      class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
      onclick={goMergeMode}
      disabled={submitting || !serverRow || diffCols.length === 0}>
      フィールドごとに merge…
     </button>
     <button
      type="button"
      class="rounded bg-primary-500 px-3 py-1 text-xs text-white hover:bg-primary-600 disabled:opacity-60"
      onclick={() => void submitMine()}
      disabled={submitting || !serverRow}>自分の編集を強制</button>
    {:else}
     <button
      type="button"
      class="me-auto rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
      onclick={() => (phase = 'summary')}
      disabled={submitting}>← 戻る</button>
     <button
      type="button"
      class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
      onclick={resetMerge}
      disabled={submitting}>全 mine にリセット</button>
     <button
      type="button"
      class="rounded bg-primary-500 px-3 py-1 text-xs text-white hover:bg-primary-600 disabled:opacity-60"
      onclick={() => void submitMerge()}
      disabled={submitting || !serverRow}>merge を保存</button>
    {/if}
   </footer>
  </div>
 </div>
{/if}
