<script lang="ts">
 import { onMount } from 'svelte';
 import { api } from './api';
 import type { ControlEvent, ControlEventHistoryItem, ControlEventKind } from './types';

 type HistoryScope = 'all' | 'flowgraph' | 'runtime_mode' | 'managed_app';

 let loading = $state(false);
 let error = $state<string | null>(null);
 let events = $state<ControlEventHistoryItem[]>([]);
 let scope = $state<HistoryScope>('all');

 const kindByScope: Record<Exclude<HistoryScope, 'all'>, ControlEventKind[]> = {
  flowgraph: ['flowgraph_reloaded', 'restart_recommended'],
  runtime_mode: ['runtime_mode_changed', 'runtime_mode_managed_apps'],
  managed_app: ['managed_app_state', 'runtime_mode_managed_apps'],
 };

 const filtered = $derived.by(() => {
  if (scope === 'all') return events;
  const kinds = new Set(kindByScope[scope]);
  return events.filter((item) => kinds.has(item.event.kind));
 });

 async function load(): Promise<void> {
  loading = true;
  error = null;
  try {
   const res = await api.eventHistory(200);
   events = res.events;
  } catch (e) {
   error = e instanceof Error ? e.message : String(e);
  } finally {
   loading = false;
  }
 }

 function summarize(ev: ControlEvent): string {
  switch (ev.kind) {
   case 'flowgraph_reloaded':
    return `${ev.ok ? 'ok' : 'issues'} nodes=${ev.node_count} errors=${ev.error_count} warnings=${ev.warning_count}`;
   case 'restart_recommended':
    return ev.reason;
   case 'runtime_mode_changed':
    return `${ev.previous_effective_id || '(none)'} -> ${ev.current_effective_id || '(none)'}`;
   case 'runtime_mode_managed_apps':
    return ev.ops.map((op) => `${op.op}:${op.id}:${op.ok ? 'ok' : 'failed'}`).join(', ') || 'no ops';
   case 'managed_app_state':
    return `${ev.id} ${ev.running ? 'running' : 'stopped'} pids=[${ev.pids.join(',')}]`;
   case 'pause_state':
    return `${ev.target} ${ev.paused ? 'paused' : 'resumed'}`;
   case 'reloaded':
    return `${ev.target}${ev.id ? `:${ev.id}` : ''}`;
   case 'processor_invoked':
    return `${ev.feature} ${ev.outcome} ${ev.elapsed_ms}ms`;
   case 'channel_datum':
    return `${ev.phase} #${ev.id} ${ev.channel}`;
   case 'oauth_status':
    return `${ev.account} ${ev.status}`;
   case 'restarting':
    return `pid=${ev.new_pid} graceful=${ev.graceful_ms}ms`;
   case 'heartbeat':
    return ev.now;
   case 'lagged':
    return `dropped ${ev.dropped}`;
  }
 }

 onMount(() => {
  void load();
 });
</script>

<section class="rounded border border-surface-200-800 bg-surface-50-950">
 <div class="flex flex-wrap items-center justify-between gap-2 border-b border-surface-200-800 px-3 py-2">
  <div>
   <h3 class="text-sm font-semibold">イベント履歴</h3>
   <p class="text-xs opacity-60">運用確認用の server-side ring buffer。</p>
  </div>
  <div class="flex items-center gap-2">
   <select class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 text-xs" bind:value={scope}>
    <option value="all">すべて</option>
    <option value="flowgraph">Flowgraph</option>
    <option value="runtime_mode">Runtime Mode</option>
    <option value="managed_app">連携アプリ</option>
   </select>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-100-900 disabled:opacity-50"
    disabled={loading}
    onclick={() => void load()}
   >
    {loading ? '読み込み中...' : '更新'}
   </button>
  </div>
 </div>

 {#if error}
  <div class="px-3 py-3 text-sm text-error-500">{error}</div>
 {:else if filtered.length === 0}
  <div class="px-3 py-6 text-center text-sm opacity-60">まだイベント履歴はありません。</div>
 {:else}
  <ol class="max-h-72 overflow-y-auto">
   {#each filtered.toReversed() as item (`${item.at}:${item.event.kind}:${summarize(item.event)}`)}
    <li class="border-b border-surface-200-800 px-3 py-2 last:border-b-0">
     <div class="flex items-center gap-2 text-[11px] opacity-60">
      <span>{new Date(item.at).toLocaleString()}</span>
      <code>{item.event.kind}</code>
     </div>
     <div class="mt-1 font-mono text-xs break-all">{summarize(item.event)}</div>
    </li>
   {/each}
  </ol>
 {/if}
</section>
