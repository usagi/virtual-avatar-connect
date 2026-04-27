<script lang="ts">
 /**
  * Legacy tools tab kept for compatibility with old routes.
  *
  * Current navigation keeps connection checks and future developer utilities under Settings.
  */
 import PingStatus from '../PingStatus.svelte';
 import WidgetSlot from '../WidgetSlot.svelte';
</script>

<div class="space-y-4">
 <section class="rounded-lg border border-surface-200-800 bg-surface-100-900 p-4">
  <h2 class="mb-2 text-sm font-semibold opacity-80">Connection</h2>
  <PingStatus />
 </section>
 <WidgetSlot
  title="Developer utilities"
  subtitle="planned"
  placeholder="Log download, runtime diagnostics, and rare maintenance operations will be grouped here."
 />
</div>
