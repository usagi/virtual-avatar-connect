<script lang="ts">
 import { onMount } from 'svelte';
 import { api } from '../api';
 import {
  buildDisplayModes,
  formatControlApiError,
  joinList,
  managedDirectiveRows,
 } from '../control/runtimeModes';
 import {
  type ModeTransitionPlan,
  type RuntimeModeManagedAppOp,
  type RuntimeModeTransitionStatus,
 } from '../types';

 let configuredModeIds = $state<string[]>([]);
 let currentModeId = $state<string | null>(null);
 let selectedModeId = $state('streaming');
 let loading = $state(true);
 let loadError = $state<string | null>(null);
 let mutating = $state(false);
 let mutationError = $state<string | null>(null);
 let planLoading = $state(false);
 let planError = $state<string | null>(null);
 let transitionPlan = $state<ModeTransitionPlan | null>(null);
 let transitionStatus = $state<RuntimeModeTransitionStatus | null>(null);
 let managedAppOps = $state<RuntimeModeManagedAppOp[]>([]);
 let planRequestSeq = 0;

 const displayModes = $derived(buildDisplayModes(configuredModeIds));

 const selectedMode = $derived(displayModes.find((m) => m.id === selectedModeId) ?? displayModes[0]);
 const managedDesiredRows = $derived(managedDirectiveRows(transitionPlan));
 const selectedCanTransit = $derived(selectedMode?.configured === true);
 const isCurrentSelected = $derived(selectedMode?.id === currentModeId);
 const transitionPercent = $derived.by(() => {
  if (!transitionStatus || transitionStatus.step_count <= 0) return 0;
  return Math.min(100, Math.round((transitionStatus.step_index / transitionStatus.step_count) * 100));
 });
 const transitDisabled = $derived(
  loading ||
   mutating ||
   transitionStatus?.active === true ||
   planLoading ||
   loadError !== null ||
   !selectedMode ||
   !selectedCanTransit ||
   isCurrentSelected,
 );
 const transitLabel = $derived.by(() => {
  if (transitionStatus?.active) return '遷移中...';
  if (mutating) return '遷移要求中...';
  if (planLoading) return '計画中...';
  if (loadError) return '遷移不可';
  if (!selectedMode?.configured) return '先に mode を設定';
  if (isCurrentSelected) return '現在の mode';
  return '遷移';
 });

 onMount(() => {
  void refreshModes();
  void refreshTransitionStatus();
  const timer = window.setInterval(() => {
   void refreshTransitionStatus();
  }, 1200);
  return () => window.clearInterval(timer);
 });

 $effect(() => {
  const target = selectedMode?.configured ? selectedMode.id : null;
  const current = currentModeId;
  if (!target || loadError) {
   planRequestSeq += 1;
   planLoading = false;
   transitionPlan = null;
   planError = null;
   return;
  }
  void current;
  void refreshTransitionPlan(target);
 });

 function modeCardClass(selected: boolean): string {
  return [
   'vac-panel p-4 text-left transition-colors',
   selected
    ? 'border-primary-500 bg-primary-500/10'
    : 'hover:bg-surface-100-900',
  ].join(' ');
 }

 async function refreshModes(): Promise<void> {
  loading = true;
  loadError = null;
  try {
   const [list, current] = await Promise.all([api.modesList(), api.currentMode()]);
   configuredModeIds = list.mode_ids;
   currentModeId = current.mode;
   if (current.mode) selectedModeId = current.mode;
   else if (list.mode_ids.length > 0) selectedModeId = list.mode_ids[0];
  } catch (e) {
   loadError = formatControlApiError(e);
  } finally {
   loading = false;
  }
 }

 async function transitSelectedMode(): Promise<void> {
  if (transitDisabled || !selectedMode) return;
  mutating = true;
  mutationError = null;
  try {
   const next = await api.modeTransit({
    mode: selectedMode.id,
    dry_run: false,
    reason: 'gui',
   });
   currentModeId = next.mode;
   transitionPlan = next.plan;
   managedAppOps = next.managed_apps ?? [];
   await refreshTransitionStatus();
  } catch (e) {
   mutationError = formatControlApiError(e);
  } finally {
   mutating = false;
  }
 }

 async function refreshTransitionStatus(): Promise<void> {
  try {
   const status = await api.modeTransition();
   transitionStatus = status;
   if (!status.active && status.phase === 'completed') {
    if (status.plan) transitionPlan = status.plan;
    if (status.managed_apps.length > 0) managedAppOps = status.managed_apps;
   }
  } catch {
   // The main Runtime Mode API status already reports backend availability.
  }
 }

 async function refreshTransitionPlan(target: string): Promise<void> {
  const seq = ++planRequestSeq;
  planLoading = true;
  planError = null;
  try {
   const plan = await api.modePlan({ target });
   if (seq !== planRequestSeq) return;
   transitionPlan = plan;
  } catch (e) {
   if (seq !== planRequestSeq) return;
   transitionPlan = null;
   planError = formatControlApiError(e);
  } finally {
   if (seq === planRequestSeq) planLoading = false;
  }
 }

</script>

<section class="grid gap-4">
 <div class="flex flex-wrap items-start justify-between gap-3">
  <div>
   <h2 class="text-xl font-semibold">Modes</h2>
   <p class="text-sm opacity-65">Runtime Mode の切替と確認</p>
  </div>
  <div class="vac-panel px-3 py-2 text-xs">
   <span class="opacity-60">Backend:</span>
   <span
    class="ml-1 font-semibold"
    class:text-warning-500={loading}
    class:text-error-500={loadError !== null}
    class:text-success-500={!loading && loadError === null}
   >
    {loading ? 'loading' : loadError ? 'error' : 'available'}
   </span>
  </div>
 </div>

 <div class="vac-panel px-4 py-3 text-sm">
  <span class="font-semibold">現在の mode:</span>
  <code class="ml-2 rounded bg-surface-100-900 px-1.5 py-0.5">{currentModeId ?? '(default)'}</code>
  <span class="ml-3 opacity-65">設定済み modes: {configuredModeIds.length}</span>
 </div>

 {#if loadError}
  <div class="rounded border border-error-500 bg-error-100-900 px-4 py-3 text-sm text-error-900-100">
   Runtime Mode API に接続できません: {loadError}
  </div>
 {/if}
 {#if mutationError}
  <div class="rounded border border-error-500 bg-error-100-900 px-4 py-3 text-sm text-error-900-100">
   Runtime Mode の遷移に失敗しました: {mutationError}
  </div>
 {/if}
 {#if transitionStatus && transitionStatus.phase !== 'idle'}
  <section class="vac-panel px-4 py-3">
   <div class="flex flex-wrap items-center justify-between gap-2">
    <div>
     <div class="text-sm font-semibold">遷移進捗</div>
     <div class="mt-1 text-xs opacity-65">
      {transitionStatus.message}
      <span class="ml-2 font-mono">{transitionStatus.phase}</span>
     </div>
    </div>
    <div class="text-xs opacity-65">
     step {transitionStatus.step_index}/{transitionStatus.step_count}
    </div>
   </div>
   <div class="mt-3 h-2 overflow-hidden rounded bg-surface-200-800">
    <div
     class="h-full {transitionStatus.phase === 'failed' ? 'bg-error-500' : 'bg-primary-500'}"
     style={`width: ${transitionPercent}%`}
    ></div>
   </div>
   {#if transitionStatus.error}
    <div class="mt-2 text-xs text-error-500">{transitionStatus.error}</div>
   {/if}
  </section>
 {/if}

 <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
  <div class="grid gap-3 md:grid-cols-2">
   {#each displayModes as mode (mode.id)}
    {@const selected = selectedMode?.id === mode.id}
    <button
     type="button"
     class={modeCardClass(selected)}
     aria-pressed={selected}
     onclick={() => (selectedModeId = mode.id)}
    >
     <div class="flex items-center justify-between gap-3">
      <div class="text-sm font-semibold">{mode.label}</div>
      <div class="rounded bg-surface-200-800 px-1.5 py-0.5 text-[10px] uppercase opacity-70">
       {mode.configured ? '設定済み' : '予定'}
      </div>
     </div>
     <p class="mt-2 text-xs leading-relaxed opacity-70">{mode.description}</p>
     <div class="mt-3 flex flex-wrap gap-1">
      {#each mode.flowgraphGroups.slice(0, 3) as group (group)}
       <code class="rounded bg-surface-100-900 px-1.5 py-0.5 text-[10px]">{group}</code>
      {/each}
     </div>
    </button>
   {/each}
  </div>

  <aside class="vac-panel">
   <div class="vac-panel-header px-4 py-2 text-sm font-semibold">
    遷移プレビュー
   </div>
   <div class="grid gap-4 p-4 text-sm">
    {#if selectedMode}
    <div>
     <div class="text-xs uppercase tracking-wide opacity-60">Target</div>
     <div class="mt-1 text-lg font-semibold">{selectedMode.label}</div>
     <p class="mt-1 text-xs opacity-70">{selectedMode.description}</p>
    </div>

    <div>
     <div class="mb-1 text-xs font-semibold opacity-70">Flowgraph desired state</div>
     {#if transitionPlan}
      <dl class="grid grid-cols-[64px_minmax(0,1fr)] gap-x-2 gap-y-1 text-xs">
       <dt class="opacity-60">Enable</dt>
       <dd class="truncate font-mono">{joinList(transitionPlan.target_flowgraph_groups.enable)}</dd>
       <dt class="opacity-60">Disable</dt>
       <dd class="truncate font-mono">{joinList(transitionPlan.target_flowgraph_groups.disable)}</dd>
      </dl>
     {:else}
      <div class="flex flex-wrap gap-1">
       {#each selectedMode.flowgraphGroups as group (group)}
        <code class="rounded bg-surface-100-900 px-1.5 py-0.5 text-xs">{group}</code>
       {/each}
      </div>
     {/if}
    </div>

    {#if selectedMode.configured}
     <div class="vac-panel-muted p-3">
      <div class="mb-2 flex items-center justify-between gap-2">
       <div class="text-xs font-semibold opacity-70">Dry-run plan</div>
       <div class="text-[10px] uppercase opacity-55">{planLoading ? 'loading' : transitionPlan?.noop ? 'noop' : 'change'}</div>
      </div>
      {#if planError}
       <div class="text-xs text-error-500">{planError}</div>
      {:else if transitionPlan}
       <dl class="grid grid-cols-[76px_minmax(0,1fr)] gap-x-2 gap-y-1 text-xs">
        <dt class="opacity-60">From</dt>
        <dd class="truncate font-mono">{transitionPlan.from_effective_id || '(none)'}</dd>
        <dt class="opacity-60">To</dt>
        <dd class="truncate font-mono">{transitionPlan.to_effective_id || '(none)'}</dd>
        <dt class="opacity-60">Enable +</dt>
        <dd class="truncate font-mono">
         {transitionPlan.flowgraph_enable_added_vs_from.length > 0
          ? transitionPlan.flowgraph_enable_added_vs_from.join(', ')
          : '-'}
        </dd>
        <dt class="opacity-60">Disable +</dt>
        <dd class="truncate font-mono">
         {transitionPlan.flowgraph_disable_added_vs_from.length > 0
          ? transitionPlan.flowgraph_disable_added_vs_from.join(', ')
          : '-'}
        </dd>
       </dl>
      {:else}
       <div class="text-xs opacity-60">Dry-run plan はありません。</div>
      {/if}
     </div>
    {/if}

    <div>
     <div class="mb-1 text-xs font-semibold opacity-70">Managed App desired state</div>
     {#if managedDesiredRows.length > 0}
      <dl class="grid grid-cols-[64px_minmax(0,1fr)] gap-x-2 gap-y-1 text-xs">
       {#each managedDesiredRows as row (row.label)}
        <dt class="opacity-60">{row.label}</dt>
        <dd class="truncate font-mono">{joinList(row.values)}</dd>
       {/each}
      </dl>
     {:else if transitionPlan}
      <div class="vac-subtle-row px-2 py-1 text-xs opacity-60">Managed App の変更はありません。</div>
     {:else}
      <ul class="grid gap-1">
       {#each selectedMode.managedApps as action (action)}
        <li class="vac-subtle-row px-2 py-1 text-xs">{action}</li>
       {/each}
      </ul>
     {/if}
    </div>

    <div>
     <div class="mb-1 text-xs font-semibold opacity-70">Notifications</div>
     <code class="rounded bg-surface-100-900 px-1.5 py-0.5 text-xs">{selectedMode.notifications}</code>
    </div>

    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1.5 text-xs hover:bg-surface-100-900 disabled:opacity-55 disabled:hover:bg-transparent"
     disabled={transitDisabled}
     title={selectedMode.configured
      ? 'Runtime Mode 遷移を要求'
      : 'この予定 mode はまだ conf [modes] にありません'}
     onclick={() => void transitSelectedMode()}
    >
     {transitLabel}
    </button>
    {#if managedAppOps.length > 0}
     <div>
      <div class="mb-1 text-xs font-semibold opacity-70">Last Managed App ops</div>
      <ul class="grid gap-1">
       {#each managedAppOps as op (`${op.op}:${op.id}:${op.ok}:${op.detail ?? ''}`)}
        <li class="vac-subtle-row px-2 py-1 text-xs">
         <span class="font-mono">{op.op}</span>
         <span class="ml-1">{op.id}</span>
         <span class={op.ok ? 'ml-2 text-success-500' : 'ml-2 text-error-500'}>{op.ok ? 'ok' : 'failed'}</span>
        </li>
       {/each}
      </ul>
     </div>
    {/if}
    {:else}
     <div class="px-2 py-6 text-center text-sm opacity-60">Runtime Mode 候補がありません。</div>
    {/if}
   </div>
  </aside>
 </div>
</section>
