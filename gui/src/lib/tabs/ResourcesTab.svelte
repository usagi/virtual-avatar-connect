<script lang="ts">
 /**
  * Runtime resources.
  *
  * External services, credentials, and child processes live here. Profile
  * management and developer diagnostics belong to Settings.
  */
 import OAuthPanel from '../OAuthPanel.svelte';
 import PausePanel from '../PausePanel.svelte';
 import ResourceOverviewPanel from '../resources/ResourceOverviewPanel.svelte';
 import VmcStatusPanel from '../resources/VmcStatusPanel.svelte';
 import RunWithEditorPanel from './RunWithEditorPanel.svelte';
</script>

<div class="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
 <div class="space-y-4">
  <RunWithEditorPanel />
  <PausePanel />
 </div>
 <div class="space-y-4">
  <ResourceOverviewPanel />
  <VmcStatusPanel />
  <OAuthPanel />
 </div>
</div>

