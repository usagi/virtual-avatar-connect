<script lang="ts">
 /**
  * Setup タブ（γ-1 の骨組み）。既存の細々した運用パネルをここに集約し、
  * γ-4 でプロファイル管理を統合した。
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
   title="Voice / AI / Twitch 詳細設定"
   subtitle="Pipeline タブへ統合済み（γ-3b）"
   placeholder="各 processor / AI persona の詳細プロパティは Pipeline タブ上でノードを選択して編集できます。"
  />
 </div>
</div>
