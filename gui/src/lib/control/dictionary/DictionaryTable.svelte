<script lang="ts">
 /**
  * Phase φ-3b: 辞書 11 カラム一覧テーブル。
  *
  * - 11 カラムの schema（source / replacement / kind / priority / is_locked / enabled
  *   / by / created_at / expires_at / tags / note）を表示。
  * - is_locked=true は背景グレー + 南京錠アイコン + 編集/削除ボタン disabled。
  * - expires_at が過去の行は全体をグレーアウト（判定は wall-clock now）。
  * - enabled=false は opacity を下げて非アクティブ表示。
  * - ソート（列ヘッダクリックで昇降切替）と文字列フィルタ（source/replacement/tags/note）を備える。
  * - 編集 / 削除 / 追加 のクリックは prop の callback にフォワード。実際の API 呼出しは
  *   親（DictionaryEditorPane + EntryForm）側で φ-3c 以降に繋ぎ込む。
  *
  * schema の参照元:
  *   [flowgraph.example/dictionary/sample.dict.tsv](../../../../../../flowgraph.example/dictionary/sample.dict.tsv)
  */

 import type { TableEntryDto, TableFileDto } from '../../types';

 type Props = {
  table: TableFileDto;
  onEdit?: (row: TableEntryDto) => void;
  onDelete?: (row: TableEntryDto) => void;
  onAdd?: () => void;
  onReload?: () => void;
 };

 let { table, onEdit, onDelete, onAdd, onReload }: Props = $props();

 type SortKey =
  | 'row_index'
  | 'source'
  | 'replacement'
  | 'kind'
  | 'priority'
  | 'enabled'
  | 'is_locked'
  | 'by'
  | 'created_at'
  | 'expires_at';

 let sortKey = $state<SortKey>('row_index');
 let sortAsc = $state(true);
 let filterText = $state('');
 let kindFilter = $state<'all' | 'literal' | 'regex'>('all');
 let hideDisabled = $state(false);
 let hideExpired = $state(false);

 function pickString(row: TableEntryDto, col: string): string {
  const v = row.values[col];
  if (v == null) return '';
  if (typeof v === 'string') return v;
  return String(v);
 }

 function pickBool(row: TableEntryDto, col: string): boolean {
  const v = row.values[col];
  if (typeof v === 'boolean') return v;
  if (typeof v === 'string') return v === 'true' || v === '1';
  if (typeof v === 'number') return v !== 0;
  return false;
 }

 function pickNumber(row: TableEntryDto, col: string): number {
  const v = row.values[col];
  if (typeof v === 'number') return v;
  if (typeof v === 'string') {
   const n = Number(v);
   return Number.isFinite(n) ? n : 0;
  }
  return 0;
 }

 function isExpired(row: TableEntryDto): boolean {
  const raw = pickString(row, 'expires_at').trim();
  if (!raw) return false;
  const t = Date.parse(raw);
  if (!Number.isFinite(t)) return false;
  return t < Date.now();
 }

 function toggleSort(key: SortKey): void {
  if (sortKey === key) {
   sortAsc = !sortAsc;
  } else {
   sortKey = key;
   sortAsc = true;
  }
 }

 const filtered = $derived.by((): TableEntryDto[] => {
  const q = filterText.trim().toLowerCase();
  const rows = table.rows.filter((r) => {
   if (kindFilter !== 'all') {
    const k = pickString(r, 'kind');
    if (k !== kindFilter) return false;
   }
   if (hideDisabled && !pickBool(r, 'enabled')) return false;
   if (hideExpired && isExpired(r)) return false;
   if (!q) return true;
   const hay = [
    pickString(r, 'source'),
    pickString(r, 'replacement'),
    pickString(r, 'tags'),
    pickString(r, 'note'),
    pickString(r, 'by')
   ]
    .join('\u0001')
    .toLowerCase();
   return hay.includes(q);
  });
  const sorted = [...rows].sort((a, b) => cmp(a, b, sortKey));
  if (!sortAsc) sorted.reverse();
  return sorted;
 });

 function cmp(a: TableEntryDto, b: TableEntryDto, key: SortKey): number {
  if (key === 'row_index') return a.row_index - b.row_index;
  if (key === 'priority') return pickNumber(a, 'priority') - pickNumber(b, 'priority');
  if (key === 'enabled' || key === 'is_locked') {
   return Number(pickBool(a, key)) - Number(pickBool(b, key));
  }
  return pickString(a, key).localeCompare(pickString(b, key));
 }

 function sortIndicator(key: SortKey): string {
  if (sortKey !== key) return '';
  return sortAsc ? ' ▲' : ' ▼';
 }

 function formatExpires(row: TableEntryDto): string {
  const raw = pickString(row, 'expires_at').trim();
  if (!raw) return '—';
  const t = Date.parse(raw);
  if (!Number.isFinite(t)) return raw;
  const diffMs = t - Date.now();
  if (diffMs < 0) return `Expired (${raw})`;
  const days = Math.round(diffMs / 86_400_000);
  if (days > 1) return `${raw} (in ${days}d)`;
  const hours = Math.round(diffMs / 3_600_000);
  return `${raw} (in ${hours}h)`;
 }
</script>

<div class="flex flex-col gap-2">
 <!-- ツールバー -->
 <div class="flex flex-wrap items-center gap-2 text-xs">
  <input
   type="search"
   class="min-w-[18rem] flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
   placeholder="source / replacement / tags / note / by で検索…"
   bind:value={filterText}
  />
  <label class="inline-flex items-center gap-1">
   kind
   <select class="rounded border border-surface-300-700 bg-surface-50-950 px-1 py-0.5 text-xs" bind:value={kindFilter}>
    <option value="all">all</option>
    <option value="literal">literal</option>
    <option value="regex">regex</option>
   </select>
  </label>
  <label class="inline-flex items-center gap-1">
   <input type="checkbox" bind:checked={hideDisabled} /> disabled を隠す
  </label>
  <label class="inline-flex items-center gap-1">
   <input type="checkbox" bind:checked={hideExpired} /> expired を隠す
  </label>
  <div class="ms-auto flex items-center gap-2">
   <span class="opacity-60">
    {filtered.length} / {table.rows.length} 行
   </span>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-2 py-1 hover:bg-surface-100-900"
    onclick={() => onReload?.()}
    disabled={!onReload}
    title="ディスクから再読込">
    ↻ Reload
   </button>
   <button
    type="button"
    class="rounded bg-primary-500 px-2 py-1 text-white hover:bg-primary-600 disabled:opacity-50"
    onclick={() => onAdd?.()}
    disabled={!onAdd || !table.editable}>
    + 追加
   </button>
  </div>
 </div>

 {#if !table.editable}
  <div class="rounded bg-warning-100-900 px-2 py-1 text-xs text-warning-700-300">
   この Table は conf.toml で <code>editable = false</code> が指定されています。閲覧のみ可能です。
  </div>
 {/if}

 <div class="overflow-x-auto rounded border border-surface-300-700">
  <table class="w-full min-w-[64rem] border-collapse text-left text-xs">
   <thead class="sticky top-0 bg-surface-100-900 text-[0.65rem] uppercase opacity-80">
    <tr>
     <th class="cursor-pointer px-2 py-1" onclick={() => toggleSort('row_index')}>
      #{sortIndicator('row_index')}
     </th>
     <th class="cursor-pointer px-2 py-1" onclick={() => toggleSort('source')}>
      source{sortIndicator('source')}
     </th>
     <th class="cursor-pointer px-2 py-1" onclick={() => toggleSort('replacement')}>
      replacement{sortIndicator('replacement')}
     </th>
     <th class="cursor-pointer px-2 py-1" onclick={() => toggleSort('kind')}>
      kind{sortIndicator('kind')}
     </th>
     <th class="cursor-pointer px-2 py-1 text-right" onclick={() => toggleSort('priority')}>
      prio{sortIndicator('priority')}
     </th>
     <th class="cursor-pointer px-2 py-1 text-center" onclick={() => toggleSort('is_locked')}>
      🔒{sortIndicator('is_locked')}
     </th>
     <th class="cursor-pointer px-2 py-1 text-center" onclick={() => toggleSort('enabled')}>
      on{sortIndicator('enabled')}
     </th>
     <th class="cursor-pointer px-2 py-1" onclick={() => toggleSort('by')}>
      by{sortIndicator('by')}
     </th>
     <th class="cursor-pointer px-2 py-1" onclick={() => toggleSort('created_at')}>
      created{sortIndicator('created_at')}
     </th>
     <th class="cursor-pointer px-2 py-1" onclick={() => toggleSort('expires_at')}>
      expires{sortIndicator('expires_at')}
     </th>
     <th class="px-2 py-1">tags</th>
     <th class="px-2 py-1">note</th>
     <th class="px-2 py-1 text-right">操作</th>
    </tr>
   </thead>
   <tbody>
    {#if filtered.length === 0}
     <tr>
      <td colspan="13" class="px-2 py-4 text-center opacity-60">
       該当する行がありません
      </td>
     </tr>
    {/if}
    {#each filtered as r (r.row_index)}
     {@const locked = pickBool(r, 'is_locked')}
     {@const enabled = pickBool(r, 'enabled')}
     {@const expired = isExpired(r)}
     {@const kind = pickString(r, 'kind')}
     <tr
      class="border-t border-surface-200-800 hover:bg-surface-50-950"
      class:bg-surface-200-800={locked}
      class:opacity-60={expired || !enabled}
     >
      <td class="px-2 py-1 font-mono text-[0.65rem] opacity-60">{r.row_index}</td>
      <td class="px-2 py-1 font-mono">{pickString(r, 'source')}</td>
      <td class="px-2 py-1">{pickString(r, 'replacement')}</td>
      <td class="px-2 py-1">
       <span
        class="rounded px-1 py-0.5 text-[0.6rem] font-medium"
        class:bg-primary-200={kind === 'literal'}
        class:text-primary-900={kind === 'literal'}
        class:bg-tertiary-200={kind === 'regex'}
        class:text-tertiary-900={kind === 'regex'}
       >
        {kind || '—'}
       </span>
      </td>
      <td class="px-2 py-1 text-right font-mono">{pickNumber(r, 'priority')}</td>
      <td class="px-2 py-1 text-center" title={locked ? 'ロック済み（削除・編集不可）' : ''}>
       {locked ? '🔒' : ''}
      </td>
      <td class="px-2 py-1 text-center">
       {enabled ? '✓' : '—'}
      </td>
      <td class="px-2 py-1 font-mono text-[0.7rem] opacity-70">{pickString(r, 'by')}</td>
      <td class="px-2 py-1 font-mono text-[0.7rem] opacity-70">{pickString(r, 'created_at')}</td>
      <td class="px-2 py-1 font-mono text-[0.7rem]" class:text-error-500={expired}>
       {formatExpires(r)}
      </td>
      <td class="px-2 py-1 text-[0.7rem] opacity-80">{pickString(r, 'tags')}</td>
      <td class="px-2 py-1 text-[0.7rem] opacity-80" title={pickString(r, 'note')}>
       {pickString(r, 'note')}
      </td>
      <td class="px-2 py-1 text-right">
       <div class="inline-flex gap-1">
        <button
         type="button"
         class="rounded border border-surface-300-700 px-1.5 py-0.5 hover:bg-surface-100-900 disabled:opacity-40"
         onclick={() => onEdit?.(r)}
         disabled={!onEdit || !table.editable || locked}
         title={locked ? 'ロックされています' : '編集'}
        >
         編集
        </button>
        <button
         type="button"
         class="rounded border border-error-400 px-1.5 py-0.5 text-error-700-300 hover:bg-error-100-900 disabled:opacity-40"
         onclick={() => onDelete?.(r)}
         disabled={!onDelete || !table.editable || locked}
         title={locked ? 'ロックされています' : '削除'}
        >
         削除
        </button>
       </div>
      </td>
     </tr>
    {/each}
   </tbody>
  </table>
 </div>
</div>
