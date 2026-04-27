<script lang="ts">
 /**
  * Runtime resources.
  *
  * External services, credentials, and child processes live here. Profile
  * management and developer diagnostics belong to Settings.
  */
 import OAuthPanel from '../OAuthPanel.svelte';
 import PausePanel from '../PausePanel.svelte';
 import WidgetSlot from '../WidgetSlot.svelte';
 import RunWithEditorPanel from './RunWithEditorPanel.svelte';
</script>

<div class="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
 <div class="space-y-4">
  <RunWithEditorPanel />
  <PausePanel />
 </div>
 <div class="space-y-4">
  <OAuthPanel />
  <WidgetSlot
   title="Avatar / OBS / TTS connectors"
   subtitle="planned resources"
   placeholder="OBS, Warudo, TTS, and avatar bridges will live here as external resources coordinated by Runtime Mode."
  />
 </div>
</div>

