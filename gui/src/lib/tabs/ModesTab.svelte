<script lang="ts">
 type PlannedMode = {
  id: string;
  label: string;
  description: string;
  flowgraphGroups: string[];
  managedApps: string[];
  notifications: string;
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

 let selectedModeId = $state('streaming');
 const selectedMode = $derived(plannedModes.find((m) => m.id === selectedModeId) ?? plannedModes[0]);

 function modeCardClass(selected: boolean): string {
  return [
   'rounded border p-4 text-left transition-colors',
   selected
    ? 'border-primary-500 bg-primary-500/10'
    : 'border-surface-200-800 bg-surface-50-950 hover:bg-surface-100-900',
  ].join(' ');
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
   <span class="ml-1 font-semibold text-warning-500">not available</span>
  </div>
 </div>

 <div class="rounded border border-warning-500/45 bg-warning-500/10 px-4 py-3 text-sm">
  Runtime Mode backend is not available in this branch. This surface is reserved for
  transition preview, mode switching, and Flowgraph / Managed App desired-state changes.
 </div>

 <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
  <div class="grid gap-3 md:grid-cols-2">
   {#each plannedModes as mode}
    {@const selected = selectedMode.id === mode.id}
    <button
     type="button"
     class={modeCardClass(selected)}
     aria-pressed={selected}
     onclick={() => (selectedModeId = mode.id)}
    >
     <div class="flex items-center justify-between gap-3">
      <div class="text-sm font-semibold">{mode.label}</div>
      <div class="rounded bg-surface-200-800 px-1.5 py-0.5 text-[10px] uppercase opacity-70">
       planned
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
     class="rounded border border-surface-300-700 px-3 py-1.5 text-xs opacity-55"
     disabled
     title="Runtime Mode backend is not implemented yet"
    >
     Transit unavailable
    </button>
   </div>
  </aside>
 </div>
</section>
