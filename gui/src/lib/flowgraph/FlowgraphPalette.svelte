<script lang="ts">
 /**
  * Phase δ-6e: ノードパレット（右上）。
  *
  * - `flowgraphStore.catalog` をカテゴリ別にグルーピングして表示。
  * - 検索ボックスで title / feature / category に対するインクリメンタル絞り込み。
  * - クリックで「現在のキャンバス中央付近」に new node を追加（`flowgraphStore.addCatalogNodeAt`）。
  * - Phase ο-6: カテゴリ単位の表示/非表示（localStorage 永続化）、HTML5 DnD で feature を dataTransfer に載せる。
  */
 import { onMount } from 'svelte';
 import { SvelteSet } from 'svelte/reactivity';
 import { flowgraphStore } from '../flowgraphStore.svelte';
 import type { FlowgraphNodeSpec } from '../types';
 import { capabilitySummary, effectClassLabel, effectTooltip } from './effectMetadata';

 const LS_HIDDEN = 'vac-flowgraph-palette-hidden-categories';

 function shortFeature(feature: string): string {
  const parts = feature.split('.');
  if (parts.length <= 1) return feature;
  return parts[parts.length - 1];
 }

 function effectBadgeClass(effectClass: FlowgraphNodeSpec['effect_class']): string {
  const base = 'rounded border px-1 py-px text-[0.58rem] font-semibold uppercase leading-none';
  switch (effectClass) {
   case 'pure':
    return `${base} border-emerald-500/40 bg-emerald-500/10 text-emerald-700`;
   case 'stateful':
    return `${base} border-sky-500/40 bg-sky-500/10 text-sky-700`;
   case 'effectful':
    return `${base} border-amber-500/50 bg-amber-500/10 text-amber-700`;
   default:
    return `${base} border-surface-300-700 bg-surface-100-900 text-surface-600-400`;
  }
 }

 let search = $state('');
 const searchLower = $derived(search.trim().toLowerCase());
 const userEnumPaletteExtra = $derived(
  flowgraphStore.currentFile?.parsed?.enums?.reduce((s, e) => s + e.variants.length, 0) ?? 0,
 );

 /** 非表示カテゴリ（`spec.category` 文字列キー）。 */
 const hiddenCategories = new SvelteSet<string>();

 onMount(() => {
  try {
   const raw = localStorage.getItem(LS_HIDDEN);
   if (!raw) return;
   const arr = JSON.parse(raw) as unknown;
   if (Array.isArray(arr)) {
    hiddenCategories.clear();
    for (const x of arr) {
     if (typeof x === 'string') hiddenCategories.add(x);
    }
   }
  } catch {
   /* ignore */
  }
 });

 function persistHidden(): void {
  try {
   localStorage.setItem(LS_HIDDEN, JSON.stringify([...hiddenCategories]));
  } catch {
   /* ignore */
  }
 }

 function toggleCategoryVisibility(category: string) {
  if (hiddenCategories.has(category)) hiddenCategories.delete(category);
  else hiddenCategories.add(category);
  persistHidden();
 }

 const groupedForDisplay = $derived.by(() => {
  const base = flowgraphStore.groupedCatalog();
  const rows = base.map((g) => {
   const specs = !searchLower
    ? g.specs
    : g.specs.filter(
      (s) =>
       s.feature.toLowerCase().includes(searchLower) ||
       s.title.toLowerCase().includes(searchLower) ||
       s.category.toLowerCase().includes(searchLower) ||
       (s.description ?? '').toLowerCase().includes(searchLower),
     );
   return {
    category: g.category,
    specs,
    hidden: hiddenCategories.has(g.category),
   };
  });
  return rows.filter((row) => row.specs.length > 0 || row.hidden);
 });

 function onAdd(spec: FlowgraphNodeSpec) {
  flowgraphStore.addCatalogNodeAt(spec, null);
 }

 function onPaletteDragStart(ev: DragEvent, spec: FlowgraphNodeSpec) {
  ev.dataTransfer?.setData('application/x-vac-flowgraph-feature', spec.feature);
  ev.dataTransfer?.setData('text/plain', spec.feature);
  if (ev.dataTransfer) ev.dataTransfer.effectAllowed = 'copy';
 }
</script>

<div class="flex h-full flex-col">
 <div class="border-b border-surface-200-800 p-2">
  <div class="mb-1 flex items-center justify-between">
   <span class="text-xs font-semibold uppercase tracking-wider opacity-60">Palette</span>
   <span class="text-xs opacity-50">
    {(flowgraphStore.catalog?.count ?? 0) + userEnumPaletteExtra} ノード
   </span>
  </div>
  <input
   type="text"
   class="w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs"
   placeholder="検索（feature / title）…"
   bind:value={search}
  />
 </div>
 <div class="flex-1 overflow-y-auto text-xs">
  {#if flowgraphStore.catalogState === 'loading'}
   <div class="p-3 opacity-60">Loading…</div>
  {:else if groupedForDisplay.length === 0}
   <div class="p-3 opacity-60">該当なし</div>
  {:else}
   {#each groupedForDisplay as group (group.category)}
    <div class="flex items-center justify-between gap-1 px-2 py-1 font-semibold uppercase tracking-wider opacity-60">
     <span>{group.category}{group.hidden ? '（非表示）' : ''}</span>
     <button
      type="button"
      class="shrink-0 rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.6rem] font-normal normal-case hover:bg-surface-200-800"
      title={group.hidden ? 'カテゴリを再表示' : 'カテゴリをパレットから隠す'}
      data-testid="palette-category-toggle"
      data-category={group.category}
      onclick={() => toggleCategoryVisibility(group.category)}
     >
      {group.hidden ? '表示' : '隠す'}
     </button>
    </div>
    {#if !group.hidden}
     <ul class="mb-1">
      {#each group.specs as spec (spec.palette_key ?? `${spec.feature}\t${spec.title}`)}
       <li>
        <button
         type="button"
         class="w-full cursor-pointer rounded px-2 py-1 text-left hover:bg-surface-200-800"
         title={[spec.feature, spec.description, effectTooltip(spec)].filter(Boolean).join('\n')}
         draggable="true"
         data-testid={`palette-entry-${spec.feature.replace(/[^a-zA-Z0-9_-]/g, '_')}`}
         onclick={() => onAdd(spec)}
         ondragstart={(e) => onPaletteDragStart(e, spec)}
        >
         <div class="flex items-baseline justify-between gap-1">
          <span class="truncate font-medium">{spec.title}</span>
          <span class="truncate text-[0.65rem] opacity-50">{shortFeature(spec.feature)}</span>
         </div>
         <div class="mt-0.5 flex min-w-0 items-center gap-1">
          <span class={effectBadgeClass(spec.effect_class)}>
           {effectClassLabel(spec.effect_class)}
          </span>
          {#if capabilitySummary(spec)}
           <span class="truncate text-[0.58rem] opacity-55">{capabilitySummary(spec)}</span>
          {/if}
         </div>
        </button>
       </li>
      {/each}
     </ul>
    {:else}
     <div class="mb-1 px-2 py-0.5 text-[0.65rem] opacity-50">このカテゴリのノード一覧は非表示です。</div>
    {/if}
   {/each}
  {/if}
 </div>
</div>
