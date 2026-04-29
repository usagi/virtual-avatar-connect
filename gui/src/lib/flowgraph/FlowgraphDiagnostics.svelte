<script lang="ts">
 /**
  * Phase δ-6e: 診断パネル（下部）。
  *
  * - `flowgraphStore.diagnostics?.diagnostics` を severity 別に色分けして表示。
  * - 行クリックで該当ファイルを `store.openFile` で開き、該当ノードを選択する（可能なら）。
 */
 import { flowgraphStore } from '../flowgraphStore.svelte';
 import type { FlowgraphDiagnostic, FlowgraphSeverity } from '../types';
 import { capabilityLabel } from './effectMetadata';

 async function onJump(d: FlowgraphDiagnostic) {
  if (!d.file) return;
  // d.file はルートからの相対パス or 絶対パス (OS 依存)。末尾の `.flowgraph.toml` を剥がして fq にする。
  // tree の既知ファイルの中から末尾マッチするものを選ぶのがいちばん頑健。
  const all = flowgraphStore.tree?.files ?? [];
  const fileNorm = String(d.file).replace(/\\/g, '/');
  const match = all.find((f) => fileNorm.endsWith(f.path));
  const fq = match?.fq ?? fileNorm.replace(/\.flowgraph\.toml$/, '').replace(/^.*\//, '');
  if (!fq) return;
  await flowgraphStore.openFile(fq);
  if (d.node) flowgraphStore.selectedNodeId = d.node;
 }

 function toneClass(s: FlowgraphSeverity): string {
  if (s === 'error') return 'text-error-500';
  if (s === 'warning') return 'text-warning-700-300';
  return 'text-surface-700-300';
 }

 function badge(s: FlowgraphSeverity): string {
  if (s === 'error') return 'bg-error-500 text-white';
  if (s === 'warning') return 'bg-warning-500 text-black';
  return 'bg-surface-400 text-white';
 }

 const diags = $derived(flowgraphStore.diagnostics?.diagnostics ?? []);
 const capabilitySummary = $derived(flowgraphStore.diagnostics?.capability_summary);
 const capabilityCounts = $derived(
  Object.entries(capabilitySummary?.capability_counts ?? {}).map(([capability, count]) => ({
   capability,
   count,
   label: capabilityLabel(capability),
  })),
 );
</script>

<div class="h-full">
 {#if capabilitySummary}
  <div class="border-b border-surface-300-700 px-2 py-1.5 text-xs">
   <div class="flex flex-wrap items-center gap-2">
    <span class="font-mono text-[0.7rem] opacity-70">
     nodes {capabilitySummary.node_count} / effects {capabilitySummary.effectful_node_count}
    </span>
    {#if capabilityCounts.length > 0}
     <span class="opacity-50">capabilities</span>
     {#each capabilityCounts as item (item.capability)}
      <span class="rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.65rem]">
       {item.label} {item.count}
      </span>
     {/each}
    {:else}
     <span class="opacity-50">capabilities none</span>
    {/if}
   </div>
  </div>
 {/if}
 {#if diags.length === 0}
  <div class="p-2 text-xs opacity-60">診断なし</div>
 {:else}
  <table class="w-full text-left text-xs">
   <thead class="sticky top-0 bg-surface-100-900">
    <tr class="text-[0.65rem] uppercase opacity-60">
     <th class="px-2 py-1">Sev</th>
     <th class="px-2 py-1">Code</th>
     <th class="px-2 py-1">Message</th>
     <th class="px-2 py-1">File</th>
     <th class="px-2 py-1">Node</th>
    </tr>
   </thead>
   <tbody>
    {#each diags as d, i (i)}
     <tr class="cursor-pointer hover:bg-surface-100-900" onclick={() => onJump(d)}>
      <td class="px-2 py-1">
       <span class={`rounded px-1.5 py-0.5 text-[0.65rem] ${badge(d.severity)}`}>
        {d.severity}
       </span>
      </td>
      <td class="px-2 py-1 font-mono text-[0.7rem]">{d.code}</td>
      <td class={`px-2 py-1 ${toneClass(d.severity)}`}>{d.message}</td>
      <td class="px-2 py-1 font-mono text-[0.7rem] opacity-70">{d.file ?? ''}</td>
      <td class="px-2 py-1 font-mono text-[0.7rem]">{d.node ?? ''}</td>
     </tr>
    {/each}
   </tbody>
  </table>
 {/if}
</div>
