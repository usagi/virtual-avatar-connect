<script lang="ts">
 import { onMount } from 'svelte';
 import { api } from '../api';
 import { eventsStore } from '../events.svelte';
 import { tabNavStore, type TabId } from '../tabs.svelte';
 import type {
  ControlEvent,
  FlowgraphDiagnosticsResponse,
  FlowgraphTreeResponse,
 ManagedAppsResponse,
 StateSnapshot,
 } from '../types';

 let snapshot = $state<StateSnapshot | null>(null);
 let currentMode = $state<string | null>(null);
 let managedApps = $state<ManagedAppsResponse | null>(null);
 let flowgraphTree = $state<FlowgraphTreeResponse | null>(null);
 let diagnostics = $state<FlowgraphDiagnosticsResponse | null>(null);
 let loading = $state(true);
 let error = $state<string | null>(null);

 async function refreshNow() {
  loading = true;
  error = null;
  try {
   const [nextSnapshot, nextMode, nextManagedApps, nextTree, nextDiagnostics] = await Promise.all([
    api.snapshot(),
    api.currentMode(),
    api.managedApps(),
    api.flowgraphTree(),
    api.flowgraphDiagnostics(),
   ]);
   snapshot = nextSnapshot;
   currentMode = nextMode.mode;
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
 const healthTone = $derived.by(() => {
  if (error || eventsStore.connection === 'error' || eventsStore.connection === 'closed') {
   return { label: 'Needs attention', className: 'text-error-500' };
  }
  if (diagnosticErrors > 0) return { label: 'Flowgraph errors', className: 'text-error-500' };
  if (diagnosticWarnings > 0) return { label: 'Warnings', className: 'text-warning-500' };
  return { label: 'Operational', className: 'text-success-500' };
 });
 const topDiagnostics = $derived(diagnostics?.diagnostics.slice(0, 5) ?? []);
 const topManagedApps = $derived(managedApps?.entries.slice(0, 6) ?? []);

 function go(tab: TabId) {
  tabNavStore.setActive(tab);
 }

 function summarizeEvent(ev: ControlEvent): string {
  switch (ev.kind) {
   case 'channel_datum':
    return `${ev.channel}: ${ev.content}`;
   case 'lagged':
    return `${ev.dropped} dropped`;
   case 'heartbeat':
    return ev.now;
   case 'pause_state':
    return ev.paused ? `${ev.target} paused` : `${ev.target} resumed`;
   case 'reloaded':
    return `${ev.target}${ev.id ? `:${ev.id}` : ''}`;
   case 'oauth_status':
    return `${ev.account} ${ev.status}`;
   case 'processor_invoked':
    return `${ev.feature} ${ev.outcome} (${ev.elapsed_ms}ms)`;
   case 'restarting':
    return `pid ${ev.current_pid} -> ${ev.new_pid}`;
   case 'managed_app_state':
    return `${ev.id} ${ev.running ? 'running' : 'stopped'}`;
   case 'flowgraph_reloaded':
    return `${ev.node_count} nodes, ${ev.error_count} errors`;
  }
 }
</script>

<section class="grid gap-4">
 <div class="flex flex-wrap items-start justify-between gap-3">
  <div>
   <h2 class="text-xl font-semibold">Now</h2>
   <p class="text-sm opacity-65">
    Runtime cockpit · <span class={healthTone.className}>{healthTone.label}</span>
   </p>
  </div>
  <div class="flex flex-wrap items-center gap-2">
   <button
    type="button"
    class="rounded border border-surface-300-700 px-3 py-1.5 text-xs hover:bg-surface-100-900"
    onclick={() => go('modes')}
   >
    Modes
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-3 py-1.5 text-xs hover:bg-surface-100-900"
    onclick={() => go('flowgraph')}
   >
    Flowgraph Studio
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-3 py-1.5 text-xs hover:bg-surface-100-900"
    onclick={() => go('resources')}
   >
    Resources
   </button>
   <button
    type="button"
    class="rounded border border-surface-300-700 px-3 py-1.5 text-xs hover:bg-surface-100-900 disabled:opacity-50"
    disabled={loading}
    onclick={refreshNow}
   >
    {loading ? 'Refreshing...' : 'Refresh'}
   </button>
  </div>
 </div>

 {#if error}
  <div class="rounded border border-error-500/50 bg-error-500/10 px-3 py-2 text-sm text-error-600-400">
   {error}
  </div>
 {/if}

 <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
  <article class="rounded border border-surface-200-800 bg-surface-50-950 p-4">
   <div class="text-xs uppercase tracking-wide opacity-60">Connection</div>
   <div class="mt-2 text-2xl font-semibold {wsTone}">{eventsStore.connection}</div>
   <div class="mt-1 text-xs opacity-60">
    {eventsStore.received_count} events / {eventsStore.dropped_count} dropped
   </div>
  </article>

  <article class="rounded border border-surface-200-800 bg-surface-50-950 p-4">
   <div class="text-xs uppercase tracking-wide opacity-60">Mode</div>
   <div class="mt-2 truncate text-2xl font-semibold" title={currentMode ?? '(default)'}>
    {currentMode ?? 'default'}
   </div>
   <button
    type="button"
    class="mt-1 text-xs text-primary-600-400 hover:underline"
    onclick={() => go('modes')}
   >
    manage
   </button>
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
    <dt class="opacity-60">Mode</dt>
    <dd class="font-mono">{currentMode ?? '(default)'}</dd>
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
   <div class="border-b border-surface-200-800 px-4 py-2 text-sm font-semibold">Managed Apps</div>
   <div class="max-h-72 overflow-y-auto p-2">
    {#if topManagedApps.length === 0}
     <div class="px-2 py-4 text-sm opacity-60">No managed apps registered.</div>
    {:else}
     <ul class="grid gap-1">
      {#each topManagedApps as app (app.id)}
       <li class="rounded bg-surface-100-900 px-2 py-1.5 text-xs">
        <div class="flex items-center justify-between gap-2">
         <span class="truncate font-medium">{app.label}</span>
         <span class={app.status.running ? 'text-success-500' : 'opacity-55'}>
          {app.status.running ? 'running' : 'stopped'}
         </span>
        </div>
       </li>
      {/each}
     </ul>
    {/if}
   </div>
  </section>
 </div>

 <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
  <section class="rounded border border-surface-200-800 bg-surface-50-950">
   <div class="flex items-center justify-between border-b border-surface-200-800 px-4 py-2">
    <span class="text-sm font-semibold">Flowgraph Problems</span>
    <button
     type="button"
     class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900"
     onclick={() => go('flowgraph')}
    >
     Open
    </button>
   </div>
   <div class="max-h-72 overflow-y-auto p-2">
    {#if topDiagnostics.length === 0}
     <div class="px-2 py-4 text-sm opacity-60">No Flowgraph diagnostics.</div>
    {:else}
     <ul class="grid gap-1">
      {#each topDiagnostics as d, i (`${d.file ?? ''}:${d.node ?? ''}:${d.code}:${i}`)}
       <li class="rounded bg-surface-100-900 px-2 py-1.5 text-xs">
        <div class="flex items-center gap-2">
         <span
          class="rounded px-1.5 py-0.5 text-[10px] uppercase"
          class:bg-error-500={d.severity === 'error'}
          class:text-white={d.severity === 'error'}
          class:bg-warning-500={d.severity === 'warning'}
          class:bg-surface-300-700={d.severity === 'info'}
         >
          {d.severity}
         </span>
         <span class="font-mono opacity-70">{d.code}</span>
        </div>
        <div class="mt-1">{d.message}</div>
       </li>
      {/each}
     </ul>
    {/if}
   </div>
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
        <div class="mt-1 truncate opacity-70" title={summarizeEvent(item.event)}>
         {summarizeEvent(item.event)}
        </div>
       </li>
      {/each}
     </ul>
    {/if}
   </div>
  </section>
 </div>
</section>
