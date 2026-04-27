<script lang="ts">
 /**
  * Resources タブ向けのリソース概要。
  *
  * ここは読み取り専用。起動・停止・再起動は Managed App ドロワーと
  * run_with エディタに残し、概要画面では状態確認に集中できるようにする。
  */
 import { onMount } from 'svelte';
 import { api } from '../api';
 import { eventsStore } from '../events.svelte';
 import { ControlApiError, type ControlEvent, type ManagedAppsResponse } from '../types';

 let loading = $state(true);
 let error = $state<string | null>(null);
 let apps = $state<ManagedAppsResponse | null>(null);

 const entries = $derived(apps?.entries ?? []);
 const total = $derived(entries.length);
 const statusTracked = $derived(entries.filter((entry) => entry.supports_status).length);
 const running = $derived(entries.filter((entry) => entry.supports_status && entry.status.running).length);
 const unknown = $derived(entries.filter((entry) => !entry.supports_status).length);
 const pidTotal = $derived(
  entries.reduce((sum, entry) => sum + (entry.supports_status ? entry.status.pids.length : 0), 0),
 );
 const recentRunning = $derived(
  entries.filter((entry) => entry.supports_status && entry.status.running).slice(0, 4),
 );

 onMount(() => {
  void refresh();
 });

 $effect(() => {
  const off = eventsStore.subscribe((ts) => {
   const ev: ControlEvent = ts.event;
   if (ev.kind !== 'managed_app_state' || !apps) return;
   const idx = apps.entries.findIndex((entry) => entry.id === ev.id);
   if (idx < 0) return;
   const next = apps.entries.slice();
   next[idx] = {
    ...next[idx],
    status: {
     id: ev.id,
     running: ev.running,
     pids: ev.pids,
     checked_at: ev.checked_at,
    },
   };
   apps = { ...apps, entries: next };
  });
  return off;
 });

 async function refresh(): Promise<void> {
  loading = true;
  error = null;
  try {
   apps = await api.managedApps();
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
</script>

<section class="rounded-lg border border-surface-200-800 bg-surface-100-900 p-4" aria-label="リソース概要">
 <header class="mb-3 flex items-start justify-between gap-3">
  <div>
   <h3 class="text-sm font-semibold opacity-80">リソース概要</h3>
   <p class="mt-0.5 text-xs opacity-60">現在の run_with 登録から Managed App の状態を確認します。</p>
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
    <div class="opacity-60">Apps</div>
   </div>
   <div class="border-r border-surface-200-800 px-2 py-2">
    <div class="font-semibold">{loading ? '-' : running}</div>
    <div class="opacity-60">起動中</div>
   </div>
   <div class="border-r border-surface-200-800 px-2 py-2">
    <div class="font-semibold">{loading ? '-' : statusTracked}</div>
    <div class="opacity-60">追跡対象</div>
   </div>
   <div class="px-2 py-2">
    <div class="font-semibold">{loading ? '-' : pidTotal}</div>
    <div class="opacity-60">PIDs</div>
   </div>
  </div>

  {#if loading}
   <p class="mt-3 text-xs opacity-60">リソース状態を読み込み中...</p>
  {:else if total === 0}
   <p class="mt-3 rounded border border-warning-500/40 bg-warning-500/10 p-2 text-xs">
    Managed App は未登録です。Runtime Mode から外部ツールを協調制御するには run_with に追加します。
   </p>
  {:else}
   <div class="mt-3 space-y-2">
    {#if unknown > 0}
     <p class="rounded border border-surface-300-700 bg-surface-50-950 p-2 text-xs opacity-75">
      {unknown} 件のアプリは起動できますが、process marker が未設定のため起動状態を報告できません。
     </p>
    {/if}
    {#if recentRunning.length > 0}
     <ul class="space-y-1">
      {#each recentRunning as entry (entry.id)}
       <li class="flex items-center justify-between gap-2 rounded bg-surface-50-950 px-2 py-1.5 text-xs">
        <span class="min-w-0 truncate font-semibold">{entry.label}</span>
        <span class="shrink-0 font-mono opacity-60">pids [{entry.status.pids.join(', ')}]</span>
       </li>
      {/each}
     </ul>
    {:else}
     <p class="mt-3 text-xs opacity-60">現在起動中の追跡対象 Managed App はありません。</p>
    {/if}
   </div>
  {/if}
 {/if}
</section>
