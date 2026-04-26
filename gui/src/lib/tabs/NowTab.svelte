<script lang="ts">
 import { onMount } from 'svelte';
 import { api } from '../api';
 import { eventsStore } from '../events.svelte';
 import type {
  FlowgraphDiagnosticsResponse,
  FlowgraphTreeResponse,
  ManagedAppsResponse,
  StateSnapshot,
 } from '../types';

 let snapshot = $state<StateSnapshot | null>(null);
 let managedApps = $state<ManagedAppsResponse | null>(null);
 let flowgraphTree = $state<FlowgraphTreeResponse | null>(null);
 let diagnostics = $state<FlowgraphDiagnosticsResponse | null>(null);
 let loading = $state(true);
 let error = $state<string | null>(null);

 async function refreshNow() {
  loading = true;
  error = null;
  try {
   const [nextSnapshot, nextManagedApps, nextTree, nextDiagnostics] = await Promise.all([
    api.snapshot(),
    api.managedApps(),
    api.flowgraphTree(),
    api.flowgraphDiagnostics(),
   ]);
   snapshot = nextSnapshot;
   managedApps = nextManagedApps;
   flowgraphTree = nextTree;
   diagnostics = nextDiagnostics;
  } catch (e) {
   error = e instanceof Error ? e.message : String(e);
  } finally {
   loading = false;
  }
 }

 onMount(() => {
  void refreshNow();
  const off = eventsStore.subscribe((ts) => {
   if (
    ts.event.kind === 'heartbeat' ||
    ts.event.kind === 'pause_state' ||
    ts.event.kind === 'managed_app_state' ||
    ts.event.kind === 'flowgraph_reloaded'
   ) {
    void refreshNow();
   }
  });
  return off;
 });

 const aiTotal = $derived(snapshot?.ai_personas.length ?? 0);
 const aiPaused = $derived(snapshot?.ai_personas.filter((a) => a.paused).length ?? 0);
 const managedTotal = $derived(managedApps?.entries.length ?? 0);
 const managedRunning = $derived(managedApps?.entries.filter((a) => a.status.running).length ?? 0);
 const flowgraphFiles = $derived(flowgraphTree?.files.length ?? 0);
 const diagnosticErrors = $derived(
  diagnostics?.diagnostics.filter((d) => d.severity === 'error').length ?? 0,
 );
 const diagnosticWarnings = $derived(
  diagnostics?.diagnostics.filter((d) => d.severity === 'warning').length ?? 0,
 );
 const recentEvents = $derived(eventsStore.recent.slice(-8).toReversed());
 const wsTone = $derived(
  eventsStore.connection === 'open'
   ? 'text-success-500'
   : eventsStore.connection === 'connecting'
    ? 'text-warning-500'
    : 'text-error-500',
 );
</script>

<section class="grid gap-4">
 <div class="flex flex-wrap items-start justify-between gap-3">
  <div>
   <h2 class="text-xl font-semibold">Now</h2>
   <p class="text-sm opacity-65">Runtime cockpit</p>
  </div>
  <button
   type="button"
   class="rounded border border-surface-300-700 px-3 py-1.5 text-xs hover:bg-surface-100-900 disabled:opacity-50"
   disabled={loading}
   onclick={refreshNow}
  >
   {loading ? 'Refreshing...' : 'Refresh'}
  </button>
 </div>

 {#if error}
  <div class="rounded border border-error-500/50 bg-error-500/10 px-3 py-2 text-sm text-error-600-400">
   {error}
  </div>
 {/if}

 <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
  <article class="rounded border border-surface-200-800 bg-surface-50-950 p-4">
   <div class="text-xs uppercase tracking-wide opacity-60">Connection</div>
   <div class="mt-2 text-2xl font-semibold {wsTone}">{eventsStore.connection}</div>
   <div class="mt-1 text-xs opacity-60">
    {eventsStore.received_count} events / {eventsStore.dropped_count} dropped
   </div>
  </article>

  <article class="rounded border border-surface-200-800 bg-surface-50-950 p-4">
   <div class="text-xs uppercase tracking-wide opacity-60">AI</div>
   <div class="mt-2 text-2xl font-semibold">{aiTotal - aiPaused} / {aiTotal}</div>
   <div class="mt-1 text-xs opacity-60">{aiPaused} paused</div>
  </article>

  <article class="rounded border border-surface-200-800 bg-surface-50-950 p-4">
   <div class="text-xs uppercase tracking-wide opacity-60">Managed Apps</div>
   <div class="mt-2 text-2xl font-semibold">{managedRunning} / {managedTotal}</div>
   <div class="mt-1 text-xs opacity-60">running</div>
  </article>

  <article class="rounded border border-surface-200-800 bg-surface-50-950 p-4">
   <div class="text-xs uppercase tracking-wide opacity-60">Flowgraph</div>
   <div class="mt-2 text-2xl font-semibold">{flowgraphFiles}</div>
   <div class="mt-1 text-xs opacity-60">
    {diagnosticErrors} errors / {diagnosticWarnings} warnings
   </div>
  </article>
 </div>

 <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
  <section class="rounded border border-surface-200-800 bg-surface-50-950">
   <div class="border-b border-surface-200-800 px-4 py-2 text-sm font-semibold">Runtime</div>
   <dl class="grid gap-x-4 gap-y-2 p-4 text-sm sm:grid-cols-[160px_minmax(0,1fr)]">
    <dt class="opacity-60">Version</dt>
    <dd class="font-mono">{snapshot?.app_version ?? '-'}</dd>
    <dt class="opacity-60">Session</dt>
    <dd class="truncate font-mono">{snapshot?.runtime.session_id ?? '-'}</dd>
    <dt class="opacity-60">Root</dt>
    <dd class="truncate font-mono">{snapshot?.runtime.root ?? '-'}</dd>
    <dt class="opacity-60">Twitch</dt>
    <dd>
     {#if snapshot?.twitch}
      {snapshot.twitch.username} -> {snapshot.twitch.channel_to}
     {:else}
      <span class="opacity-60">not configured</span>
     {/if}
    </dd>
   </dl>
  </section>

  <section class="rounded border border-surface-200-800 bg-surface-50-950">
   <div class="border-b border-surface-200-800 px-4 py-2 text-sm font-semibold">Recent Events</div>
   <div class="max-h-72 overflow-y-auto p-2">
    {#if recentEvents.length === 0}
     <div class="px-2 py-4 text-sm opacity-60">No events yet.</div>
    {:else}
     <ul class="grid gap-1">
      {#each recentEvents as item (item.seq)}
       <li class="rounded bg-surface-100-900 px-2 py-1.5 text-xs">
        <div class="flex items-center justify-between gap-2">
         <span class="font-mono">{item.event.kind}</span>
         <span class="opacity-55">{new Date(item.received_at).toLocaleTimeString()}</span>
        </div>
       </li>
      {/each}
     </ul>
    {/if}
   </div>
  </section>
 </div>
</section>

