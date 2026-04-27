<script lang="ts">
 import { onMount } from 'svelte';
 import { api } from '../api';
 import { ControlApiError } from '../types';

 type PlannedMode = {
  id: string;
  label: string;
  description: string;
  flowgraphGroups: string[];
  managedApps: string[];
  notifications: string;
 };

 type DisplayMode = PlannedMode & {
  configured: boolean;
 };

 const plannedModes: PlannedMode[] = [
  {
   id: 'daily',
   label: 'Daily',
   description: 'Assistant, alerts, and lightweight personal automations.',
   flowgraphGroups: ['assistant', 'alerts', 'rss'],
   managedApps: ['stop obs', 'stop warudo', 'stop tts'],
   notifications: 'normal',
  },
  {
   id: 'streaming',
   label: 'Streaming',
   description: 'Streaming stack, avatar tools, OBS control, and stream-safe alerts.',
   flowgraphGroups: ['assistant', 'streaming', 'avatar', 'obs'],
   managedApps: ['start obs', 'start warudo', 'start tts'],
   notifications: 'stream_safe',
  },
  {
   id: 'work',
   label: 'Work',
   description: 'Reduced interruptions while keeping critical alerts and assistant access.',
   flowgraphGroups: ['assistant', 'alerts'],
   managedApps: ['stop obs', 'stop warudo', 'stop tts'],
   notifications: 'important_only',
  },
  {
   id: 'sleep',
   label: 'Sleep',
   description: 'Only critical monitoring and emergency notification flows.',
   flowgraphGroups: ['emergency_alerts'],
   managedApps: ['stop obs', 'stop warudo', 'stop tts'],
   notifications: 'critical_only',
  },
  {
   id: 'rta',
   label: 'RTA',
   description: 'Game/run-specific timers, splits, alerts, and reduced background noise.',
   flowgraphGroups: ['game', 'timer', 'alerts'],
   managedApps: ['leave game tools', 'stop streaming extras'],
   notifications: 'run_safe',
  },
 ] as const;

 let configuredModeIds = $state<string[]>([]);
 let currentModeId = $state<string | null>(null);
 let selectedModeId = $state('streaming');
 let loading = $state(true);
 let loadError = $state<string | null>(null);
 let mutating = $state(false);
 let mutationError = $state<string | null>(null);

 const displayModes = $derived.by<DisplayMode[]>(() => {
  const configured = new Set(configuredModeIds);
  const known = new Set(plannedModes.map((m) => m.id));
  const plannedRows = plannedModes.map((m) => ({ ...m, configured: configured.has(m.id) }));
  const customRows = configuredModeIds
   .filter((id) => !known.has(id))
   .map((id) => ({
    id,
    label: id,
    description: 'Configured runtime mode from conf.',
    flowgraphGroups: ['configured'],
    managedApps: ['use configured desired state'],
    notifications: 'configured',
    configured: true,
   }));
  return [...customRows, ...plannedRows];
 });

 const selectedMode = $derived(displayModes.find((m) => m.id === selectedModeId) ?? displayModes[0]);
 const selectedCanTransit = $derived(selectedMode?.configured === true);
 const isCurrentSelected = $derived(selectedMode?.id === currentModeId);
 const transitDisabled = $derived(
  loading || mutating || loadError !== null || !selectedMode || !selectedCanTransit || isCurrentSelected,
 );
 const transitLabel = $derived.by(() => {
  if (mutating) return 'Transiting...';
  if (loadError) return 'Transit unavailable';
  if (!selectedMode?.configured) return 'Configure mode first';
  if (isCurrentSelected) return 'Current mode';
  return 'Transit';
 });

 onMount(() => {
  void refreshModes();
 });

 function modeCardClass(selected: boolean): string {
  return [
   'rounded border p-4 text-left transition-colors',
   selected
    ? 'border-primary-500 bg-primary-500/10'
   : 'border-surface-200-800 bg-surface-50-950 hover:bg-surface-100-900',
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
   loadError = formatError(e);
  } finally {
   loading = false;
  }
 }

 async function transitSelectedMode(): Promise<void> {
  if (transitDisabled || !selectedMode) return;
  mutating = true;
  mutationError = null;
  try {
   const next = await api.putCurrentMode({ mode: selectedMode.id });
   currentModeId = next.mode;
  } catch (e) {
   mutationError = formatError(e);
  } finally {
   mutating = false;
  }
 }

 function formatError(e: unknown): string {
  if (e instanceof ControlApiError) return `${e.status} ${e.statusText}`;
  if (e instanceof Error) return e.message;
  return String(e);
 }
</script>

<section class="grid gap-4">
 <div class="flex flex-wrap items-start justify-between gap-3">
  <div>
   <h2 class="text-xl font-semibold">Modes</h2>
   <p class="text-sm opacity-65">Runtime mode control</p>
  </div>
  <div class="rounded border border-surface-200-800 bg-surface-50-950 px-3 py-2 text-xs">
   <span class="opacity-60">Current backend:</span>
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

 <div class="rounded border border-surface-200-800 bg-surface-50-950 px-4 py-3 text-sm">
  <span class="font-semibold">Current mode:</span>
  <code class="ml-2 rounded bg-surface-100-900 px-1.5 py-0.5">{currentModeId ?? '(default)'}</code>
  <span class="ml-3 opacity-65">Configured modes: {configuredModeIds.length}</span>
 </div>

 {#if loadError}
  <div class="rounded border border-error-500 bg-error-100-900 px-4 py-3 text-sm text-error-900-100">
   Runtime Mode API failed: {loadError}
  </div>
 {/if}
 {#if mutationError}
  <div class="rounded border border-error-500 bg-error-100-900 px-4 py-3 text-sm text-error-900-100">
   Runtime Mode transition failed: {mutationError}
  </div>
 {/if}

 <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
  <div class="grid gap-3 md:grid-cols-2">
   {#each displayModes as mode}
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
       {mode.configured ? 'configured' : 'planned'}
      </div>
     </div>
     <p class="mt-2 text-xs leading-relaxed opacity-70">{mode.description}</p>
     <div class="mt-3 flex flex-wrap gap-1">
      {#each mode.flowgraphGroups.slice(0, 3) as group}
       <code class="rounded bg-surface-100-900 px-1.5 py-0.5 text-[10px]">{group}</code>
      {/each}
     </div>
    </button>
   {/each}
  </div>

  <aside class="rounded border border-surface-200-800 bg-surface-50-950">
   <div class="border-b border-surface-200-800 px-4 py-2 text-sm font-semibold">
    Transition Preview
   </div>
   <div class="grid gap-4 p-4 text-sm">
    {#if selectedMode}
    <div>
     <div class="text-xs uppercase tracking-wide opacity-60">Target</div>
     <div class="mt-1 text-lg font-semibold">{selectedMode.label}</div>
     <p class="mt-1 text-xs opacity-70">{selectedMode.description}</p>
    </div>

    <div>
     <div class="mb-1 text-xs font-semibold opacity-70">Flowgraph groups</div>
     <div class="flex flex-wrap gap-1">
      {#each selectedMode.flowgraphGroups as group}
       <code class="rounded bg-surface-100-900 px-1.5 py-0.5 text-xs">{group}</code>
      {/each}
     </div>
    </div>

    <div>
     <div class="mb-1 text-xs font-semibold opacity-70">Managed Apps</div>
     <ul class="grid gap-1">
      {#each selectedMode.managedApps as action}
       <li class="rounded bg-surface-100-900 px-2 py-1 text-xs">{action}</li>
      {/each}
     </ul>
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
      ? 'Request Runtime Mode transition'
      : 'This planned mode is not present in conf [modes] yet'}
     onclick={() => void transitSelectedMode()}
    >
     {transitLabel}
    </button>
    {:else}
     <div class="px-2 py-6 text-center text-sm opacity-60">No runtime mode candidate.</div>
    {/if}
   </div>
  </aside>
 </div>
</section>
