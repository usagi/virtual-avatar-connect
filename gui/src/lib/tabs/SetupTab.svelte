<script lang="ts">
 /**
  * Legacy setup tab kept for compatibility with old routes.
  * New navigation splits these controls across Resources and Settings.
  */
 import WidgetSlot from '../WidgetSlot.svelte';
 import OAuthPanel from '../OAuthPanel.svelte';
 import ReloadPanel from '../ReloadPanel.svelte';
 import PausePanel from '../PausePanel.svelte';
 import ProfileManagerPanel from './ProfileManagerPanel.svelte';
 import RunWithEditorPanel from './RunWithEditorPanel.svelte';
</script>

<div class="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
 <div class="space-y-4">
  <ProfileManagerPanel />
  <OAuthPanel />
  <ReloadPanel />
  <PausePanel />
 </div>
 <div class="space-y-4">
  <RunWithEditorPanel />
  <WidgetSlot
   title="Voice / AI / Twitch details"
   subtitle="moved to Flowgraph Studio"
   placeholder="Detailed processor and AI persona properties are edited by selecting nodes in Flowgraph Studio."
  />
 </div>
</div>
