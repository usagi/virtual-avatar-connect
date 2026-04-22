<script lang="ts">
 /**
  * Phase δ-6e: ノードパレット（右上）。
  *
  * - `flowgraphStore.catalog` をカテゴリ別にグルーピングして表示。
  * - 検索ボックスで title / feature / category に対するインクリメンタル絞り込み。
  * - クリックで「現在のキャンバス中央付近」に new node を追加する簡易 UX。
  *   HTML5 DnD も将来のために draggable="true" を付けてあるが、canvas 側の drop は本フェーズでは未実装
  *   （dblclick 代替で十分実用に耐える）。
  */
 import { flowgraphStore, type FlowgraphDraftNode } from '../flowgraphStore.svelte';
 import type { FlowgraphNodeSpec } from '../types';

 function shortFeature(feature: string): string {
  const parts = feature.split('.');
  if (parts.length <= 1) return feature;
  return parts[parts.length - 1];
 }

 let search = $state('');
 const searchLower = $derived(search.trim().toLowerCase());

 const grouped = $derived.by(() => {
  const base = flowgraphStore.groupedCatalog();
  if (!searchLower) return base;
  return base
   .map((g) => ({
    category: g.category,
    specs: g.specs.filter(
     (s) =>
      s.feature.toLowerCase().includes(searchLower) ||
      s.title.toLowerCase().includes(searchLower) ||
      s.category.toLowerCase().includes(searchLower) ||
      (s.description ?? '').toLowerCase().includes(searchLower),
    ),
   }))
   .filter((g) => g.specs.length > 0);
 });

 function uniqueId(feature: string, existing: FlowgraphDraftNode[]): string {
  const base = feature.replace(/^.*\./, '').replace(/[^a-zA-Z0-9_]/g, '_');
  let i = 1;
  let id = base;
  const used = new Set(existing.map((n) => n.id));
  while (used.has(id)) {
   i += 1;
   id = `${base}_${i}`;
  }
  return id;
 }

 function defaultProperties(spec: FlowgraphNodeSpec): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const p of spec.properties) {
   if (p.required) {
    // required なプロパティは GUI から必ず明示値を入れたいので、default があればそれを置く。
    out[p.name] = p.default;
   }
  }
  return out;
 }

 function onAdd(spec: FlowgraphNodeSpec) {
  if (!flowgraphStore.currentFq) {
   alert('先にファイルを選択してください。');
   return;
  }
  if (!flowgraphStore.draftNodes) return;
  const nodes = flowgraphStore.draftNodes;
  // 既存ノードの中央 + ランダムオフセットで配置。ない場合は (100, 100)。
  let x = 100;
  let y = 100;
  if (nodes.length > 0) {
   const withPos = nodes.filter((n) => n.position);
   if (withPos.length > 0) {
    const cx = withPos.reduce((s, n) => s + n.position![0], 0) / withPos.length;
    const cy = withPos.reduce((s, n) => s + n.position![1], 0) / withPos.length;
    x = cx + (Math.random() - 0.5) * 160;
    y = cy + (Math.random() - 0.5) * 160;
   }
  }
  const node: FlowgraphDraftNode = {
   id: uniqueId(spec.feature, nodes),
   feature: spec.feature,
   position: [Math.round(x), Math.round(y)],
   properties: defaultProperties(spec),
  };
  flowgraphStore.addNode(node);
  flowgraphStore.selectedNodeId = node.id;
 }
</script>

<div class="flex h-full flex-col">
 <div class="border-b border-surface-200-800 p-2">
  <div class="mb-1 flex items-center justify-between">
   <span class="text-xs font-semibold uppercase tracking-wider opacity-60">Palette</span>
   <span class="text-xs opacity-50">{flowgraphStore.catalog?.count ?? 0} ノード</span>
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
  {:else if grouped.length === 0}
   <div class="p-3 opacity-60">該当なし</div>
  {:else}
   {#each grouped as group (group.category)}
    <div class="px-2 py-1 font-semibold uppercase tracking-wider opacity-60">
     {group.category}
    </div>
    <ul class="mb-1">
     {#each group.specs as spec (spec.feature)}
      <li>
       <button
        type="button"
        class="w-full cursor-pointer rounded px-2 py-1 text-left hover:bg-surface-200-800"
        title={spec.feature + (spec.description ? `\n${spec.description}` : '')}
        draggable="true"
        onclick={() => onAdd(spec)}
       >
        <div class="flex items-baseline justify-between gap-1">
         <span class="truncate font-medium">{spec.title}</span>
         <span class="truncate text-[0.65rem] opacity-50">{shortFeature(spec.feature)}</span>
        </div>
       </button>
      </li>
     {/each}
    </ul>
   {/each}
  {/if}
 </div>
</div>

