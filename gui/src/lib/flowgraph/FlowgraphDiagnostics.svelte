<script lang="ts">
  /**
   * Phase δ-6e: 診断パネル（下部）。
   *
   * - `flowgraphStore.diagnostics?.diagnostics` を severity 別に色分けして表示。
   * - 行クリックで該当ファイルを `store.openFile` で開き、該当ノードを選択する（可能なら）。
   */
  import { flowgraphStore } from "../flowgraphStore.svelte";
  import type { FlowgraphDiagnostic, FlowgraphSeverity } from "../types";
  import { capabilityLabel } from "./effectMetadata";

  async function onJump(d: FlowgraphDiagnostic) {
    if (!d.file) return;
    // d.file はルートからの相対パス or 絶対パス (OS 依存)。末尾の `.flowgraph.toml` を剥がして fq にする。
    // tree の既知ファイルの中から末尾マッチするものを選ぶのがいちばん頑健。
    const all = flowgraphStore.tree?.files ?? [];
    const fileNorm = String(d.file).replace(/\\/g, "/");
    const match = all.find((f) => fileNorm.endsWith(f.path));
    const fq =
      match?.fq ??
      fileNorm.replace(/\.flowgraph\.toml$/, "").replace(/^.*\//, "");
    if (!fq) return;
    await flowgraphStore.openFile(fq);
    if (d.node) flowgraphStore.selectedNodeId = d.node;
  }

  function toneClass(s: FlowgraphSeverity): string {
    if (s === "error") return "text-error-500";
    if (s === "warning") return "text-warning-700-300";
    return "text-surface-700-300";
  }

  function badge(s: FlowgraphSeverity): string {
    if (s === "error") return "bg-error-500 text-white";
    if (s === "warning") return "bg-warning-500 text-black";
    return "bg-surface-400 text-white";
  }

  function formatJson(value: unknown): string {
    return JSON.stringify(value);
  }

  const diags = $derived(flowgraphStore.diagnostics?.diagnostics ?? []);
  const capabilitySummary = $derived(
    flowgraphStore.diagnostics?.capability_summary,
  );
  const capabilityCounts = $derived(
    Object.entries(capabilitySummary?.capability_counts ?? {}).map(
      ([capability, count]) => ({
        capability,
        count,
        label: capabilityLabel(capability),
      }),
    ),
  );
  const capabilityNodes = $derived(capabilitySummary?.nodes ?? []);
  const stateNodes = $derived(capabilitySummary?.state_nodes ?? []);
  const loadedStateSummary = $derived(
    flowgraphStore.diagnostics?.loaded_state_summary,
  );
  const loadedStateSnapshot = $derived(
    flowgraphStore.diagnostics?.loaded_state_snapshot,
  );
  const loadedStateNodes = $derived(loadedStateSummary?.nodes ?? []);
  const loadedSnapshotNodes = $derived(loadedStateSnapshot?.nodes ?? []);
</script>

<div class="h-full">
  {#if capabilitySummary}
    <div class="border-b border-surface-300-700 px-2 py-1.5 text-xs">
      <div class="flex flex-wrap items-center gap-2">
        <span class="font-mono text-[0.7rem] opacity-70">
          nodes {capabilitySummary.node_count} / effects {capabilitySummary.effectful_node_count}
          / state {capabilitySummary.stateful_node_count}
        </span>
        {#if capabilityCounts.length > 0}
          <span class="opacity-50">capabilities</span>
          {#each capabilityCounts as item (item.capability)}
            <span
              class="rounded border border-surface-300-700 px-1.5 py-0.5 text-[0.65rem]"
            >
              {item.label}
              {item.count}
            </span>
          {/each}
        {:else}
          <span class="opacity-50">capabilities none</span>
        {/if}
      </div>
      {#if capabilityNodes.length > 0}
        <details class="mt-1">
          <summary class="cursor-pointer select-none text-[0.65rem] opacity-60">
            capability nodes {capabilityNodes.length}
          </summary>
          <div class="mt-1 grid gap-1">
            {#each capabilityNodes as node (node.node)}
              <div
                class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(6rem,auto)] items-center gap-2 text-[0.65rem]"
              >
                <span class="truncate font-mono" title={node.node}
                  >{node.node}</span
                >
                <span class="truncate font-mono opacity-70" title={node.feature}
                  >{node.feature}</span
                >
                <span
                  class="truncate text-right opacity-70"
                  title={node.capabilities
                    .map((capability) => capabilityLabel(capability))
                    .join(", ") || node.effect_class}
                >
                  {node.capabilities
                    .map((capability) => capabilityLabel(capability))
                    .join(", ") || node.effect_class}
                </span>
              </div>
            {/each}
          </div>
        </details>
      {/if}
      {#if stateNodes.length > 0}
        <details class="mt-1">
          <summary class="cursor-pointer select-none text-[0.65rem] opacity-60">
            state nodes {stateNodes.length} / volatile {capabilitySummary.volatile_state_node_count}
          </summary>
          <div class="mt-1 grid gap-1">
            {#each stateNodes as node (node.node)}
              <div
                class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(8rem,auto)_minmax(8rem,auto)] items-center gap-2 text-[0.65rem]"
              >
                <span class="truncate font-mono" title={node.node}
                  >{node.node}</span
                >
                <span class="truncate font-mono opacity-70" title={node.feature}
                  >{node.feature}</span
                >
                <span
                  class="truncate text-right opacity-70"
                  title={`${node.scope} / ${node.storage} / ${node.lifetime}`}
                >
                  {node.storage} / {node.lifetime}
                </span>
                <span
                  class="truncate text-right opacity-70"
                  title={`snapshot: ${node.snapshot_policy} / ${node.snapshot_format}, restore: ${node.restore_policy}, migration: ${node.migration_policy}, persistence: ${node.persistence_policy}`}
                >
                  snapshot {node.snapshot_policy} / restore {node.restore_policy}
                </span>
              </div>
            {/each}
          </div>
        </details>
      {/if}
      {#if loadedStateSummary && (loadedStateNodes.length > 0 || loadedSnapshotNodes.length > 0)}
        <details class="mt-1">
          <summary class="cursor-pointer select-none text-[0.65rem] opacity-60">
            loaded state {loadedStateSummary.stateful_node_count} / snapshots {loadedStateSnapshot?.snapshot_node_count ??
              0}
          </summary>
          <div class="mt-1 grid gap-1">
            {#each loadedStateNodes as node (node.node)}
              <div
                class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(4rem,auto)_minmax(8rem,auto)] items-center gap-2 text-[0.65rem]"
              >
                <span class="truncate font-mono" title={node.node}
                  >{node.node}</span
                >
                <span class="truncate font-mono opacity-70" title={node.feature}
                  >{node.feature}</span
                >
                <span class="truncate text-right font-mono opacity-70"
                  >v{node.version}</span
                >
                <span
                  class="truncate text-right opacity-70"
                  title={`snapshot: ${node.state_model.snapshot_policy} / ${node.state_model.snapshot_format}, restore: ${node.state_model.restore_policy}`}
                >
                  {node.state_model.snapshot_format} / {node.state_model
                    .restore_policy}
                </span>
              </div>
            {/each}
            {#each loadedSnapshotNodes as node (node.node)}
              <div
                class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(4rem,auto)_minmax(10rem,auto)] items-center gap-2 text-[0.65rem]"
              >
                <span class="truncate font-mono opacity-70" title={node.node}
                  >{node.node}</span
                >
                <span class="truncate font-mono opacity-60" title={node.feature}
                  >{node.feature}</span
                >
                <span class="truncate text-right font-mono opacity-60"
                  >v{node.version}</span
                >
                <span
                  class="truncate text-right font-mono opacity-70"
                  title={formatJson(node.value)}
                >
                  {formatJson(node.value)}
                </span>
              </div>
            {/each}
          </div>
        </details>
      {/if}
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
          <tr
            class="cursor-pointer hover:bg-surface-100-900"
            onclick={() => onJump(d)}
          >
            <td class="px-2 py-1">
              <span
                class={`rounded px-1.5 py-0.5 text-[0.65rem] ${badge(d.severity)}`}
              >
                {d.severity}
              </span>
            </td>
            <td class="px-2 py-1 font-mono text-[0.7rem]">{d.code}</td>
            <td class={`px-2 py-1 ${toneClass(d.severity)}`}>{d.message}</td>
            <td class="px-2 py-1 font-mono text-[0.7rem] opacity-70"
              >{d.file ?? ""}</td
            >
            <td class="px-2 py-1 font-mono text-[0.7rem]">{d.node ?? ""}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>
