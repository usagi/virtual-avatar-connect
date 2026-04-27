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
   subtitle="future resources"
   placeholder="OBS、Warudo、TTS、avatar bridge などは Runtime Mode と連動する外部リソースとしてここに集約します。"
  />
 </div>
</div>

