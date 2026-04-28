<script lang="ts">
 /**
  * VMC passthrough の状態表示。
  *
  * M3 first slice は読み取り専用。転送先の追加・削除は runtime 変更 semantics を
  * 固めてから、このパネルに操作を足す。
  */
 import { onMount } from 'svelte';
 import { api } from '../api';
 import { ControlApiError, type VmcPassthroughBindState, type VmcStatusResponse } from '../types';

 let loading = $state(true);
 let error = $state<string | null>(null);
 let status = $state<VmcStatusResponse | null>(null);

 const entries = $derived(status?.entries ?? []);
 const total = $derived(entries.length);
 const running = $derived(entries.filter((entry) => entry.state === 'running').length);
 const failed = $derived(entries.filter((entry) => entry.state === 'failed').length);
 const packets = $derived(entries.reduce((sum, entry) => sum + entry.packets_received, 0));
 const bytes = $derived(entries.reduce((sum, entry) => sum + entry.bytes_received, 0));

 onMount(() => {
  void refresh();
 });

 async function refresh(): Promise<void> {
  loading = true;
  error = null;
  try {
   status = await api.vmcStatus();
  } catch (e) {
   error = formatError(e);
  } finally {
   loading = false;
  }
 }

 function formatError(e: unknown): string {
  if (e instanceof ControlApiError) return `${e.status} ${e.statusText}`;
  if (e instanceof Error) return e.message;
  return String(e);
 }

 function stateLabel(state: VmcPassthroughBindState): string {
  switch (state) {
   case 'configured':
    return '準備中';
   case 'skipped':
    return 'スキップ';
   case 'running':
    return '転送中';
   case 'failed':
    return '失敗';
   case 'stopped':
    return '停止';
   default: {
    const _exhaustive: never = state;
    return _exhaustive;
   }
  }
 }

 function stateClass(state: VmcPassthroughBindState): string {
  switch (state) {
   case 'running':
    return 'border-success-500/40 bg-success-500/10 text-success-700-300';
   case 'failed':
    return 'border-error-500/40 bg-error-500/10 text-error-700-300';
   case 'skipped':
    return 'border-warning-500/40 bg-warning-500/10 text-warning-700-300';
   default:
    return 'border-surface-300-700 bg-surface-100-900 text-surface-700-300';
  }
 }

 function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  const kib = value / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  return `${(kib / 1024).toFixed(1)} MiB`;
 }
</script>

<section class="vac-panel-muted p-4" aria-label="VMC passthrough 状態">
 <header class="mb-3 flex items-start justify-between gap-3">
  <div>
   <h3 class="text-sm font-semibold opacity-80">VMC passthrough</h3>
   <p class="mt-0.5 text-xs opacity-60">motion 層の UDP 転送状態と packet 統計を確認します。</p>
  </div>
  <button
   type="button"
   class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-200-800 disabled:opacity-50"
   disabled={loading}
   onclick={() => void refresh()}
  >
   更新
  </button>
 </header>

 {#if error}
  <p class="rounded border border-error-500/40 bg-error-500/10 p-2 text-xs">{error}</p>
 {:else}
  <div class="grid grid-cols-4 overflow-hidden rounded border border-surface-200-800 bg-surface-50-950 text-center text-xs">
   <div class="border-r border-surface-200-800 px-2 py-2">
    <div class="font-semibold">{loading ? '-' : total}</div>
    <div class="opacity-60">Routes</div>
   </div>
   <div class="border-r border-surface-200-800 px-2 py-2">
    <div class="font-semibold">{loading ? '-' : running}</div>
    <div class="opacity-60">転送中</div>
   </div>
   <div class="border-r border-surface-200-800 px-2 py-2">
    <div class="font-semibold">{loading ? '-' : failed}</div>
    <div class="opacity-60">失敗</div>
   </div>
   <div class="px-2 py-2">
    <div class="font-semibold">{loading ? '-' : packets}</div>
    <div class="opacity-60">Packets</div>
   </div>
  </div>

  {#if loading}
   <p class="mt-3 text-xs opacity-60">VMC 状態を読み込み中...</p>
  {:else if total === 0}
   <p class="mt-3 rounded border border-surface-300-700 bg-surface-100-900 p-2 text-xs opacity-75">
    VMC passthrough は未設定です。<code>[motion]</code> の <code>[[motion.vmc_passthrough]]</code> で受信口と転送先を追加します。
   </p>
  {:else}
   <div class="mt-3 space-y-2">
    <p class="text-xs opacity-60">累計転送量: {formatBytes(bytes)}</p>
    <ul class="space-y-2">
     {#each entries as entry (entry.id)}
      <li class="vac-subtle-row px-3 py-2 text-xs">
       <div class="flex items-start justify-between gap-3">
        <div class="min-w-0">
         <div class="flex flex-wrap items-center gap-2">
          <span class="font-semibold">{entry.label ?? entry.id}</span>
          <span class={`rounded border px-1.5 py-0.5 ${stateClass(entry.state)}`}>{stateLabel(entry.state)}</span>
          {#if !entry.enabled}
           <span class="rounded border border-warning-500/40 bg-warning-500/10 px-1.5 py-0.5 text-warning-700-300">disabled</span>
          {/if}
         </div>
         <div class="mt-1 font-mono opacity-65">{entry.bind} -> {entry.forward_to.join(', ')}</div>
         {#if entry.error}
          <div class="mt-1 text-error-700-300">{entry.error}</div>
         {/if}
        </div>
        <div class="shrink-0 text-right font-mono opacity-70">
         <div>{entry.packets_received} rx</div>
         <div>{entry.packets_forwarded} tx / {entry.send_errors} err</div>
        </div>
       </div>
      </li>
     {/each}
    </ul>
   </div>
  {/if}
 {/if}
</section>
