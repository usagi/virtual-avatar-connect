<script lang="ts">
 /**
  * Phase φ-3b: Dictionary Editor Pane のルート。
  *
  * - `/api/v1/control/tables` を初回に取得し、`role == "dictionary"` なエントリのみ tab 化。
  * - タブ切替で `/api/v1/control/table/{key}` を読み直して `DictionaryTable.svelte` に流し込む。
  * - 追加 / 編集 / 削除のクリックは φ-3c の `DictionaryEntryForm.svelte` / φ-3d の
  *   `DictionaryConflictDialog.svelte` で拾う。φ-3b 時点では placeholder として toast を出す。
  * - ControlEvent.reloaded でファイル変更が他クライアントから通知されたら自動再読込する予定
  *   （実装は φ-3e の LiveTab 組み込み時に WebSocket と接続）。
  */

 import { onMount } from 'svelte';
 import { dictionaryEditorStore } from './dictionaryEditorStore.svelte';
 import { toastStore } from '../../toasts.svelte';
 import DictionaryTable from './DictionaryTable.svelte';
 import type { TableEntryDto } from '../../types';

 onMount(() => {
  void dictionaryEditorStore.loadCatalog();
 });

 async function selectTab(key: string): Promise<void> {
  if (dictionaryEditorStore.currentKey === key) return;
  await dictionaryEditorStore.selectTable(key);
 }

 async function onReload(): Promise<void> {
  await dictionaryEditorStore.reloadCurrent();
  toastStore.info('再読込しました', dictionaryEditorStore.currentKey ?? undefined);
 }

 function onAdd(): void {
  toastStore.info('追加フォームは φ-3c で実装予定です');
 }

 function onEdit(row: TableEntryDto): void {
  toastStore.info('編集フォームは φ-3c で実装予定です', `row_index=${row.row_index}`);
 }

 function onDelete(row: TableEntryDto): void {
  toastStore.info('削除ハンドラは φ-3c で実装予定です', `row_index=${row.row_index}`);
 }
</script>

<section class="flex flex-col gap-3">
 <header class="flex flex-wrap items-center gap-2">
  <h3 class="text-sm font-semibold">Dictionary Editor</h3>
  <span class="text-xs opacity-60">
   conf.toml の
   <code class="rounded bg-surface-100-900 px-1">[[control_api.tables]]</code>
   で <code>role = "dictionary"</code> を指定した TSV が対象です。
  </span>
  <button
   type="button"
   class="ms-auto rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-100-900"
   onclick={() => void dictionaryEditorStore.loadCatalog()}
   disabled={dictionaryEditorStore.phase.kind === 'loading-catalog'}
  >
   ↻ カタログ再取得
  </button>
 </header>

 {#if dictionaryEditorStore.phase.kind === 'loading-catalog'}
  <div class="text-xs opacity-70">カタログをロード中…</div>
 {:else if dictionaryEditorStore.phase.kind === 'error'}
  <div class="rounded bg-error-100-900 px-2 py-1 text-xs text-error-700-300">
   {dictionaryEditorStore.phase.message}
  </div>
 {:else if dictionaryEditorStore.dictionaryTables.length === 0}
  <div class="rounded bg-surface-100-900 px-3 py-2 text-xs opacity-80">
   <code>[[control_api.tables]]</code> に <code>role = "dictionary"</code> の
   エントリが登録されていません。<br />
   例:
   <pre class="mt-1 rounded bg-surface-950-50 p-2 text-[0.65rem] leading-snug">{`[[control_api.tables]]
key       = "chat_dict"
path      = "dictionary.chat.dict.tsv"
label     = "Chat 辞書"
role      = "dictionary"
editable  = true`}</pre>
   設定を反映するには VAC の再起動が必要です。
  </div>
 {:else}
  <!-- タブ一覧 -->
  <nav class="flex flex-wrap gap-1 border-b border-surface-300-700">
   {#each dictionaryEditorStore.dictionaryTables as t (t.key)}
    {@const active = dictionaryEditorStore.currentKey === t.key}
    <button
     type="button"
     class="rounded-t border-b-2 px-3 py-1 text-xs"
     class:border-primary-500={active}
     class:font-semibold={active}
     class:border-transparent={!active}
     onclick={() => void selectTab(t.key)}
     title={t.path}
    >
     {t.label ?? t.key}
     {#if !t.exists}
      <span class="ms-1 text-warning-700-300" title="ファイル未作成（空 Table 扱い）">⚠</span>
     {/if}
     {#if !t.editable}
      <span class="ms-1 opacity-60" title="read-only">🔒</span>
     {/if}
    </button>
   {/each}
  </nav>

  <!-- 現在選択中の Table -->
  {#if dictionaryEditorStore.phase.kind === 'loading-table'}
   <div class="text-xs opacity-70">
    {dictionaryEditorStore.phase.key} を読み込み中…
   </div>
  {:else if dictionaryEditorStore.currentTable}
   <div class="flex flex-col gap-2">
    <div class="text-[0.7rem] opacity-60">
     <span class="font-mono">{dictionaryEditorStore.currentTable.path}</span>
     <span class="mx-2 opacity-40">|</span>
     hash:
     <span class="font-mono">
      b3:{dictionaryEditorStore.currentTable.content_hash.slice(0, 12)}…
     </span>
     <span class="mx-2 opacity-40">|</span>
     columns: {dictionaryEditorStore.currentTable.columns.length}
    </div>

    <DictionaryTable
     table={dictionaryEditorStore.currentTable}
     {onEdit}
     {onDelete}
     {onAdd}
     onReload={() => void onReload()}
    />
   </div>
  {/if}
 {/if}
</section>
