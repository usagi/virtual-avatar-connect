<script lang="ts">
 /**
  * Persistent Managed App drawer.
  *
  * Lists apps registered through run_with and exposes explicit start, stop,
  * restart, and minimize actions. The drawer also listens to managed_app_state
  * WebSocket events so process state changes appear without a full refresh.
  */
 import { SvelteSet } from 'svelte/reactivity';
 import { api } from './api';
 import { eventsStore } from './events.svelte';
 import { toastStore } from './toasts.svelte';
 import { ControlApiError, type ControlEvent, type ManagedAppView, type ManagedAppsResponse } from './types';

 interface Props {
  open: boolean;
 }

 let { open = $bindable() }: Props = $props();

 let loading = $state(true);
 let error = $state<string | null>(null);
 let resp = $state<ManagedAppsResponse | null>(null);
 const busyIds = new SvelteSet<string>();

 async function load(): Promise<void> {
  loading = true;
  error = null;
  try {
   resp = await api.managedApps();
  } catch (e) {
   error = formatErr(e);
  } finally {
   loading = false;
  }
 }

 $effect(() => {
  if (open) void load();
 });

 $effect(() => {
  const off = eventsStore.subscribe((ts) => {
   const ev: ControlEvent = ts.event;
   if (ev.kind !== 'managed_app_state' || !resp) return;
   const idx = resp.entries.findIndex((entry) => entry.id === ev.id);
   if (idx < 0) return;
   const next = resp.entries.slice();
   next[idx] = {
    ...next[idx],
    status: {
     id: ev.id,
     running: ev.running,
     pids: ev.pids,
     checked_at: ev.checked_at,
    },
   };
   resp = { ...resp, entries: next };
  });
  return off;
 });

 function setBusy(id: string, busy: boolean): void {
  if (busy) busyIds.add(id);
  else busyIds.delete(id);
 }

 async function actStart(entry: ManagedAppView): Promise<void> {
  setBusy(entry.id, true);
  try {
   const r = await api.managedAppStart(entry.id);
   if (r.was_running) toastStore.warn(`${entry.label} is already running.`);
   else toastStore.success(`Started ${entry.label}.`);
  } catch (e) {
   const msg = formatErr(e);
   if (e instanceof ControlApiError && e.status === 409) toastStore.warn(`${entry.label} is already running.`, msg);
   else toastStore.error(`Failed to start ${entry.label}.`, msg);
  } finally {
   setBusy(entry.id, false);
  }
 }

 async function actStop(entry: ManagedAppView): Promise<void> {
  const ok = confirm(
   `Stop ${entry.label}?\n\nVAC will request close, wait briefly, then force-terminate remaining tracked PIDs if needed.`,
  );
  if (!ok) return;
  setBusy(entry.id, true);
  try {
   const r = await api.managedAppStop(entry.id, { grace_ms: 3000 });
   if (r.terminated_pids > 0) {
    toastStore.warn(
     `Force-stopped ${entry.label}.`,
     `closed=${r.closed_windows}, terminated=${r.terminated_pids}`,
    );
   } else {
    toastStore.success(`Requested close for ${entry.label}.`, `closed_windows=${r.closed_windows}`);
   }
  } catch (e) {
   toastStore.error(`Failed to stop ${entry.label}.`, formatErr(e));
  } finally {
   setBusy(entry.id, false);
  }
 }

 async function actRestart(entry: ManagedAppView): Promise<void> {
  const ok = confirm(`Restart ${entry.label}?\n\nVAC will stop the tracked process first, then start it again.`);
  if (!ok) return;
  setBusy(entry.id, true);
  try {
   const r = await api.managedAppRestart(entry.id, { grace_ms: 3000 });
   const detail = r.was_running
    ? `stopped closed=${r.closed_windows}, terminated=${r.terminated_pids}`
    : 'was not running before restart';
   toastStore.success(`Restarted ${entry.label}.`, detail);
  } catch (e) {
   toastStore.error(`Failed to restart ${entry.label}.`, formatErr(e));
  } finally {
   setBusy(entry.id, false);
  }
 }

 async function actMinimize(entry: ManagedAppView): Promise<void> {
  setBusy(entry.id, true);
  try {
   const r = await api.managedAppMinimize(entry.id);
   toastStore.info(`Queued minimize for ${entry.label}.`, `pids=${r.scheduled_pids}`);
  } catch (e) {
   toastStore.error(`Failed to minimize ${entry.label}.`, formatErr(e));
  } finally {
   setBusy(entry.id, false);
  }
 }

 function formatErr(e: unknown): string {
  if (e instanceof ControlApiError) return `${e.status} ${e.statusText}`;
  if (e instanceof Error) return e.message;
  return String(e);
 }

 function statusLabel(entry: ManagedAppView): string {
  if (!entry.supports_status) return 'untracked';
  return entry.status.running ? 'running' : 'stopped';
 }

 function statusClass(entry: ManagedAppView): string {
  if (!entry.supports_status) return 'bg-surface-300-700 text-surface-900-100';
  return entry.status.running
   ? 'bg-success-300-700 text-success-900-100'
   : 'bg-error-300-700/60 text-error-900-100';
 }

 function close(): void {
  open = false;
 }

 function onBackdropKey(e: KeyboardEvent): void {
  if (e.key === 'Escape') close();
 }
</script>

{#if open}
 <div
  class="fixed inset-0 z-40 bg-surface-950/40 backdrop-blur-sm"
  role="button"
  tabindex="-1"
  aria-label="Close managed apps drawer"
  onclick={close}
  onkeydown={onBackdropKey}
 ></div>
 <aside
  class="fixed right-0 top-0 z-50 flex h-screen w-full max-w-md flex-col border-l border-surface-200-800 bg-surface-50-950 shadow-xl"
  aria-label="Managed Apps"
 >
  <header class="flex items-center justify-between border-b border-surface-200-800 px-4 py-3">
   <div>
    <h2 class="text-sm font-semibold">Managed Apps</h2>
    <p class="text-xs opacity-60">Monitor and control apps registered through run_with.</p>
   </div>
   <div class="flex items-center gap-2">
    <button
     type="button"
     class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900"
     onclick={() => void load()}
     disabled={loading}
    >
     Refresh
    </button>
    <button
     type="button"
     class="rounded border border-surface-300-700 px-2 py-0.5 text-xs hover:bg-surface-100-900"
     aria-label="Close"
     onclick={close}
    >
     x
    </button>
   </div>
  </header>

  <div class="flex-1 overflow-y-auto p-3">
   {#if loading}
    <p class="text-xs opacity-70">Loading...</p>
   {:else if error}
    <p class="rounded border border-error-500/40 bg-error-500/10 p-2 text-xs">{error}</p>
   {:else if !resp || resp.entries.length === 0}
    <p class="rounded border border-warning-500/40 bg-warning-500/10 p-2 text-xs">
     No Managed Apps are registered.<br />
     Add <code>run_with</code> entries with <code>if_not_running</code> markers to track process state.
    </p>
   {:else}
    <ul class="space-y-2">
     {#each resp.entries as entry (entry.id)}
      {@const busy = busyIds.has(entry.id)}
      {@const canStop = entry.supports_status && entry.status.running}
      {@const canMinimize = entry.supports_status && entry.status.running}
      <li class="rounded border border-surface-200-800 bg-surface-100-900 p-3">
       <div class="flex items-start justify-between gap-2">
        <div class="min-w-0 flex-1">
         <div class="flex items-center gap-2">
          <span class="truncate text-sm font-semibold">{entry.label}</span>
          <span class="rounded px-1.5 py-0 text-[10px] font-semibold uppercase {statusClass(entry)}">
           {statusLabel(entry)}
          </span>
         </div>
         <p class="mt-0.5 truncate font-mono text-[11px] opacity-60" title={entry.command}>
          {entry.command}
         </p>
         <div class="mt-1 flex flex-wrap gap-1 text-[10px] opacity-70">
          <code>id={entry.id}</code>
          {#if entry.process_marker}
           <code>marker={entry.process_marker}</code>
          {/if}
          {#if entry.minimized}
           <code>minimized</code>
          {/if}
          {#if entry.run_as_admin}
           <code class="text-warning-500">admin</code>
          {/if}
          {#if entry.supports_status && entry.status.running && entry.status.pids.length > 0}
           <code>pids=[{entry.status.pids.join(',')}]</code>
          {/if}
         </div>
        </div>
       </div>

       <div class="mt-2 flex flex-wrap gap-1.5">
        <button
         type="button"
         class="rounded bg-primary-500 px-2 py-1 text-xs font-semibold text-white hover:bg-primary-600 disabled:opacity-50"
         onclick={() => void actStart(entry)}
         disabled={busy || (entry.supports_status && entry.status.running)}
        >
         Start
        </button>
        <button
         type="button"
         class="rounded border border-error-500 px-2 py-1 text-xs font-semibold text-error-500 hover:bg-error-500 hover:text-white disabled:opacity-30"
         onclick={() => void actStop(entry)}
         disabled={busy || !canStop}
        >
         Stop
        </button>
        <button
         type="button"
         class="rounded border border-warning-500 px-2 py-1 text-xs font-semibold text-warning-500 hover:bg-warning-500 hover:text-white disabled:opacity-30"
         onclick={() => void actRestart(entry)}
         disabled={busy || !entry.supports_status}
         title="Stop and then start the tracked app"
        >
         Restart
        </button>
        <button
         type="button"
         class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-200-800 disabled:opacity-30"
         onclick={() => void actMinimize(entry)}
         disabled={busy || !canMinimize}
         title="Windows-only minimize request"
        >
         Minimize
        </button>
       </div>
      </li>
     {/each}
    </ul>
   {/if}
  </div>
 </aside>
{/if}
