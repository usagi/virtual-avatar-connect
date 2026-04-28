<script lang="ts">
 /**
  * Phase φ-4: V2 Dictionary Live Quick-Add Widget。
  *
  * V1 の `DictionaryQuickAddWidget.svelte`（modify processor 時代）を Flowgraph Trigger
  * ベースで作り直した後継。`[[control_api.tables]].quick_add = {...}` を持つ Table の
  * 数だけ tab 化し、1 行フォーム (source + → + replacement + [Learn]) で
  * `POST /api/v1/control/flowgraph/{inst}/trigger/{quick_add.node_id}` を発火する。
  *
  * UI:
  *   - 上段: 対象テーブル tab + kind toggle + source / replacement + [Learn]
  *   - 下段: 直近 10 件の履歴（per-key）、各行に [Undo]（forget_node_id 設定時のみ）
  *
  * 永続化:
  *   - `vac.dictQuickAdd.target_key`         : 選択中 table key
  *   - `vac.dictQuickAdd.history.<key>`      : 履歴配列（JSON）
  *
  * 再読込:
  *   - learn 成功時に `dictionaryEditorStore.reloadCurrent()` を呼び、同じ key の
  *     Editor Pane が開いていれば即時反映させる（flowgraph が table.write_tsv まで
  *     配線されていることが前提。未配線なら reload しても何も変わらないが害はない）。
  *
  * 参考:
  *   - spec: docs/roadmap/phase-phi-control-api-dictionary-editor.md §7
  *   - 依存: api.triggerFlowgraphNode / dictionaryEditorStore.catalog
  */

 import { onMount } from 'svelte';
 import { api } from '../../api';
 import { ControlApiError } from '../../types';
 import type { TableCatalogItem } from '../../types';
 import { dictionaryEditorStore, extractControlApiMessage } from './dictionaryEditorStore.svelte';
 import { toastStore } from '../../toasts.svelte';

 const LS_TARGET = 'vac.dictQuickAdd.target_key';
 const LS_HISTORY_PREFIX = 'vac.dictQuickAdd.history.';
 const HISTORY_MAX = 10;

 type HistoryItem = {
  seq: number;
  target_key: string;
  node_id: string;
  forget_node_id: string | null;
  source: string;
  replacement: string;
  kind: 'literal' | 'regex';
  at: string;
  /** undo 済み（forget trigger 成功）ならフラグを立てて一覧から gray-out。 */
  undone?: boolean;
 };

 let targetKey = $state<string>(localStorage.getItem(LS_TARGET) ?? '');
 let inputSource = $state('');
 let inputReplacement = $state('');
 let kindOverride = $state<'literal' | 'regex' | null>(null);
 let submitting = $state(false);
 let undoing = $state<number | null>(null);
 let historyOpen = $state(false);

 // 履歴は per-key で localStorage に保存、in-memory は Map で保持。
 let historyByKey = $state<Record<string, HistoryItem[]>>({});
 let nextSeq = 1;

 // Editor Pane と同じ store のカタログを使う。
 const quickAddTables: TableCatalogItem[] = $derived(
  dictionaryEditorStore.catalog.filter((t) => t.quick_add != null)
 );

 const selectedTable: TableCatalogItem | null = $derived(
  quickAddTables.find((t) => t.key === targetKey) ?? null
 );

 const effectiveKind = $derived.by((): 'literal' | 'regex' => {
  if (kindOverride) return kindOverride;
  const k = (selectedTable?.quick_add?.kind ?? 'literal').toLowerCase();
  return k === 'regex' ? 'regex' : 'literal';
 });

 const history: HistoryItem[] = $derived(
  targetKey ? (historyByKey[targetKey] ?? []) : []
 );

 const canLearn = $derived(
  selectedTable != null && !!inputSource.trim() && !!inputReplacement.trim() && !submitting
 );

 onMount(() => {
  // カタログがまだロードされてなければキック（Pane 側と二重実行になってもキャッシュに乗るだけで害なし）。
  if (dictionaryEditorStore.catalog.length === 0) {
   void dictionaryEditorStore.loadCatalog();
  }
 });

 // カタログが変化したら、選択 key の妥当性と履歴のロードを再評価する。
 $effect(() => {
  if (quickAddTables.length === 0) return;
  if (!targetKey || !quickAddTables.some((t) => t.key === targetKey)) {
   targetKey = quickAddTables[0].key;
  }
 });

 $effect(() => {
  if (!targetKey) return;
  localStorage.setItem(LS_TARGET, targetKey);
  if (!(targetKey in historyByKey)) {
   historyByKey = { ...historyByKey, [targetKey]: loadHistory(targetKey) };
  }
 });

 function loadHistory(key: string): HistoryItem[] {
  try {
   const raw = localStorage.getItem(LS_HISTORY_PREFIX + key);
   if (!raw) return [];
   const arr = JSON.parse(raw) as HistoryItem[];
   if (!Array.isArray(arr)) return [];
   // seq の最大値を nextSeq に反映（複数 key をまたいでも衝突しないよう +1 後に Math.max）。
   for (const h of arr) if (h.seq >= nextSeq) nextSeq = h.seq + 1;
   return arr;
  } catch {
   return [];
  }
 }

 function persistHistory(key: string, items: HistoryItem[]): void {
  try {
   localStorage.setItem(LS_HISTORY_PREFIX + key, JSON.stringify(items));
  } catch {
   // localStorage 満杯等は UX を壊さないように無視。
  }
 }

 function pushHistory(item: HistoryItem): void {
  const prev = historyByKey[item.target_key] ?? [];
  const next = [item, ...prev].slice(0, HISTORY_MAX);
  historyByKey = { ...historyByKey, [item.target_key]: next };
  persistHistory(item.target_key, next);
 }

 function updateHistoryItem(target_key: string, seq: number, patch: Partial<HistoryItem>): void {
  const prev = historyByKey[target_key] ?? [];
  const next = prev.map((h) => (h.seq === seq ? { ...h, ...patch } : h));
  historyByKey = { ...historyByKey, [target_key]: next };
  persistHistory(target_key, next);
 }

 async function onSubmit(): Promise<void> {
  if (!canLearn || !selectedTable || !selectedTable.quick_add) return;
  const source = inputSource.trim();
  const replacement = inputReplacement.trim();
  const kind = effectiveKind;
  const nodeId = selectedTable.quick_add.node_id;
  const forgetNodeId = selectedTable.quick_add.forget_node_id ?? null;
  submitting = true;
  try {
   await api.triggerFlowgraphNode(nodeId, {
    inputs: {
     source,
     replacement,
     kind,
     by: 'gui:quick_add',
    },
   });
   toastStore.success('Learn トリガを送信しました', `${source} → ${replacement}`);
   pushHistory({
    seq: nextSeq++,
    target_key: selectedTable.key,
    node_id: nodeId,
    forget_node_id: forgetNodeId,
    source,
    replacement,
    kind,
    at: new Date().toISOString(),
   });
   inputSource = '';
   inputReplacement = '';
   // Editor Pane と同じ key を開いていれば自動で反映されるよう再読込。
   if (dictionaryEditorStore.currentKey === selectedTable.key) {
    // flowgraph 側が write_tsv まで配線されていない場合は変化しないが、その時は何もしない。
    void dictionaryEditorStore.reloadCurrent();
   }
  } catch (e) {
   handleTriggerError(e, 'Learn トリガに失敗しました');
  } finally {
   submitting = false;
  }
 }

 async function onUndo(item: HistoryItem): Promise<void> {
  if (!item.forget_node_id || item.undone || undoing != null) return;
  undoing = item.seq;
  try {
   await api.triggerFlowgraphNode(item.forget_node_id, {
    inputs: {
     source: item.source,
     replacement: item.replacement,
     mode: 'latest',
    },
   });
   updateHistoryItem(item.target_key, item.seq, { undone: true });
   toastStore.success('Forget トリガを送信しました', `${item.source} → ${item.replacement}`);
   if (dictionaryEditorStore.currentKey === item.target_key) {
    void dictionaryEditorStore.reloadCurrent();
   }
  } catch (e) {
   handleTriggerError(e, 'Forget トリガに失敗しました');
  } finally {
   undoing = null;
  }
 }

 function handleTriggerError(e: unknown, title: string): void {
  if (e instanceof ControlApiError) {
   // 400: control_triggerable=false、404: instance/node unknown、422: 型不一致 など。
   toastStore.error(title, extractControlApiMessage(e));
  } else if (e instanceof Error) {
   toastStore.error(title, e.message);
  } else {
   toastStore.error(title, String(e));
  }
 }

 function onKeyDown(e: KeyboardEvent): void {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
   void onSubmit();
  }
 }

 function clearHistory(): void {
  if (!targetKey) return;
  if (!confirm('この Table の Quick-Add 履歴をすべて削除しますか？（既に Learn 済みの行は消えません）')) return;
  historyByKey = { ...historyByKey, [targetKey]: [] };
  persistHistory(targetKey, []);
 }
</script>

<section class="rounded border border-surface-200-800 bg-surface-50-950 p-2 text-xs">
 <header class="mb-2 flex flex-wrap items-center gap-2">
  <h3 class="text-xs font-semibold">Live Quick-Add</h3>
  <span class="opacity-60">
   `glossary.learn` ノードに 1-shot trigger を発火して配信中に即語録を追加します。
  </span>
  {#if quickAddTables.length > 1}
   <select
    class="ms-auto rounded border border-surface-300-700 bg-surface-50-950 px-1 py-0.5 text-xs"
    bind:value={targetKey}
   >
    {#each quickAddTables as t (t.key)}
     <option value={t.key}>{t.label ?? t.key}</option>
    {/each}
   </select>
  {/if}
 </header>

 {#if quickAddTables.length === 0}
  <p class="opacity-70">
   <code>[[control_api.tables]]</code> に
   <code>quick_add = &#123; node_id = &quot;...&quot; &#125;</code>
   を指定した Table がありません。conf.toml を見直してください。
  </p>
 {:else if !selectedTable}
  <p class="opacity-70">テーブルを選択してください。</p>
 {:else}
  <div class="flex flex-wrap items-center gap-2">
   <span class="opacity-60 text-[0.7rem]">{selectedTable.label ?? selectedTable.key}</span>
   <label class="inline-flex items-center gap-1 text-[0.7rem]">
    kind
    <select
     class="rounded border border-surface-300-700 bg-surface-50-950 px-1 py-0.5 text-xs"
     value={kindOverride ?? ''}
     onchange={(e) => {
      const v = (e.currentTarget as HTMLSelectElement).value;
      kindOverride = v === 'literal' || v === 'regex' ? v : null;
     }}
    >
     <option value="">
      既定（{selectedTable.quick_add?.kind ?? 'literal'}）
     </option>
     <option value="literal">literal</option>
     <option value="regex">regex</option>
    </select>
   </label>
   <input
    type="text"
    class="min-w-[8rem] flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
    placeholder="source（聞こえた音 / 置換元）"
    bind:value={inputSource}
    onkeydown={onKeyDown}
    disabled={submitting}
   />
   <span aria-hidden="true" class="opacity-60">→</span>
   <input
    type="text"
    class="min-w-[8rem] flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
    placeholder="replacement（正しい読み / 置換先）"
    bind:value={inputReplacement}
    onkeydown={onKeyDown}
    disabled={submitting}
   />
   <button
    type="button"
    class="rounded bg-primary-500 px-3 py-1 text-xs text-white hover:bg-primary-600 disabled:opacity-50"
    onclick={() => void onSubmit()}
    disabled={!canLearn}
    title="Ctrl+Enter でも送信できます"
   >
    {submitting ? '送信中…' : 'Learn'}
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-100-900"
    onclick={() => (historyOpen = !historyOpen)}
    title="直近 {HISTORY_MAX} 件"
   >
    履歴 ({history.filter((h) => !h.undone).length})
    <span class="ms-1 opacity-60">{historyOpen ? '▲' : '▼'}</span>
   </button>
  </div>

  {#if !selectedTable.quick_add?.forget_node_id}
   <p class="mt-1 text-[0.65rem] opacity-50">
    <code>forget_node_id</code> 未設定のため [Undo] は無効です。
    conf.toml の <code>quick_add</code> に追加してください。
   </p>
  {/if}

  {#if historyOpen}
   <div class="mt-2 rounded border border-surface-300-700">
    <div class="flex items-center border-b border-surface-300-700 bg-surface-100-900 px-2 py-1 text-[0.65rem] uppercase opacity-70">
     <span>履歴（this table）</span>
     <button
      type="button"
      class="ms-auto rounded px-1 py-0.5 hover:bg-surface-50-950"
      onclick={clearHistory}
     >
      clear
     </button>
    </div>
    {#if history.length === 0}
     <p class="px-2 py-2 opacity-60">履歴はまだありません</p>
    {:else}
     <ul class="max-h-48 overflow-y-auto text-[0.7rem]">
      {#each history as h (h.seq)}
       <li
        class="flex items-center gap-2 border-b border-surface-200-800 px-2 py-1 last:border-b-0"
        class:opacity-50={h.undone}
       >
        <span class="rounded bg-surface-200-800 px-1 text-[0.6rem] uppercase">
         {h.kind}
        </span>
        <span class="font-mono">{h.source}</span>
        <span class="opacity-60">→</span>
        <span>{h.replacement}</span>
        <span class="ms-auto font-mono text-[0.65rem] opacity-50">
         {new Date(h.at).toLocaleTimeString()}
        </span>
        <button
         type="button"
         class="rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.65rem] hover:bg-surface-100-900 disabled:opacity-40"
         onclick={() => void onUndo(h)}
         disabled={!h.forget_node_id || h.undone || undoing != null}
         title={h.undone
          ? 'Undo 済み'
          : h.forget_node_id
           ? 'forget トリガを送る'
           : 'forget_node_id 未設定'}
        >
         {undoing === h.seq ? '…' : h.undone ? 'Undone' : 'Undo'}
        </button>
       </li>
      {/each}
     </ul>
    {/if}
   </div>
  {/if}
 {/if}
</section>
