<script lang="ts">
 const plannedModes = [
  { id: 'daily', label: 'Daily', state: 'planned' },
  { id: 'streaming', label: 'Streaming', state: 'planned' },
  { id: 'work', label: 'Work', state: 'planned' },
  { id: 'sleep', label: 'Sleep', state: 'planned' },
  { id: 'rta', label: 'RTA', state: 'planned' },
 ] as const;
</script>

<section class="grid gap-4">
 <div>
  <h2 class="text-xl font-semibold">Modes</h2>
  <p class="text-sm opacity-65">Runtime mode control</p>
 </div>

 <div class="rounded border border-warning-500/45 bg-warning-500/10 px-4 py-3 text-sm">
  Runtime Mode backend is not available in this branch. This surface is reserved for
  transition preview, mode switching, and Flowgraph / Managed App desired-state changes.
 </div>

 <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-5">
  {#each plannedModes as mode}
   <article class="rounded border border-surface-200-800 bg-surface-50-950 p-4">
    <div class="text-sm font-semibold">{mode.label}</div>
    <div class="mt-2 text-xs uppercase tracking-wide opacity-55">{mode.state}</div>
   </article>
  {/each}
 </div>
</section>

