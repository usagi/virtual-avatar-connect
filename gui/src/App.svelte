<script lang="ts">
 /**
  * Application shell for the v2 Runtime Cockpit.
  *
  * The shell owns top-level navigation, global status, toasts, restart/shutdown
  * controls, and the persistent Managed Apps drawer. Feature surfaces stay in
  * their tab components.
  */
 import ConnectionBadge from './lib/ConnectionBadge.svelte';
 import TabNav from './lib/TabNav.svelte';
 import StatusBar from './lib/StatusBar.svelte';
 import ToastLayer from './lib/ToastLayer.svelte';
 import ToastBridge from './lib/ToastBridge.svelte';
 import RestartDialog from './lib/RestartDialog.svelte';
 import ManagedAppDrawer from './lib/ManagedAppDrawer.svelte';

 import NowTab from './lib/tabs/NowTab.svelte';
 import ModesTab from './lib/tabs/ModesTab.svelte';
 import ResourcesTab from './lib/tabs/ResourcesTab.svelte';
 import SettingsTab from './lib/tabs/SettingsTab.svelte';
 import FlowgraphTab from './lib/tabs/FlowgraphTab.svelte';
 import LogsTab from './lib/tabs/LogsTab.svelte';

 import { tabNavStore } from './lib/tabs.svelte';
 import { api } from './lib/api';
 import { toastStore } from './lib/toasts.svelte';

 let restartDialogOpen = $state(false);
 let managedAppDrawerOpen = $state(false);
 let shutdownInFlight = $state(false);

 async function handleShutdownClick(): Promise<void> {
  if (shutdownInFlight) return;
  const ok = window.confirm(
   'Shutdown VAC?\n\nVAC will also try to stop Managed Apps started through run_with. Browser connections will close.',
  );
  if (!ok) return;
  shutdownInFlight = true;
  try {
   const res = await api.shutdown();
   toastStore.info('Shutdown requested.', `pid=${res.current_pid} / status=${res.status}`);
  } catch (e) {
   shutdownInFlight = false;
   toastStore.error('Shutdown request failed.', String(e));
  }
 }
</script>

<div class="flex min-h-screen flex-col bg-surface-50-950 text-surface-950-50">
 <header class="sticky top-0 z-20 border-b border-surface-200-800 bg-surface-50-950/95 backdrop-blur">
  <div class="flex items-center justify-between gap-3 px-6 py-2">
   <div class="flex items-baseline gap-3">
    <h1 class="text-lg font-bold">
     Virtual Avatar Connect
     <span class="text-primary-500">Runtime Cockpit</span>
    </h1>
    <span class="text-xs opacity-60">v2 GUI redesign</span>
   </div>
   <div class="flex items-center gap-2">
    <ConnectionBadge />
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
     title="Open Managed Apps drawer"
     onclick={() => (managedAppDrawerOpen = true)}
    >
     Managed Apps...
    </button>
    <button
     type="button"
     class="rounded border border-surface-300-700 px-3 py-1 text-xs hover:bg-surface-100-900"
     title="Restart VAC or switch profile"
     onclick={() => (restartDialogOpen = true)}
    >
     Restart...
    </button>
    <button
     type="button"
     class="rounded border border-error-500/60 px-3 py-1 text-xs text-error-500 hover:bg-error-500/10 disabled:opacity-50"
     title="Shutdown VAC and try to stop Managed Apps"
     disabled={shutdownInFlight}
     onclick={handleShutdownClick}
    >
     {shutdownInFlight ? 'Shutting down...' : 'Shutdown'}
    </button>
   </div>
  </div>
 </header>

 <div class="grid min-h-0 flex-1 lg:grid-cols-[240px_minmax(0,1fr)]">
  <aside class="border-b border-surface-200-800 bg-surface-100-900/60 lg:border-b-0 lg:border-r">
   <TabNav />
  </aside>

  <main class="min-w-0 flex-1 px-4 py-4 lg:px-6">
   {#if tabNavStore.active === 'now'}
    <NowTab />
   {:else if tabNavStore.active === 'modes'}
    <ModesTab />
   {:else if tabNavStore.active === 'flowgraph'}
    <FlowgraphTab />
   {:else if tabNavStore.active === 'resources'}
    <ResourcesTab />
   {:else if tabNavStore.active === 'observability'}
    <LogsTab />
   {:else if tabNavStore.active === 'settings'}
    <SettingsTab />
   {/if}
  </main>
 </div>

 <StatusBar />
</div>

<ToastBridge />
<ToastLayer />
<RestartDialog bind:open={restartDialogOpen} />
<ManagedAppDrawer bind:open={managedAppDrawerOpen} />
