<script lang="ts">
 /**
  * Runtime settings and developer utilities.
  *
  * Durable profile/conf management, reload, and low-level diagnostics are kept
  * away from Resources so daily operation is less noisy.
  */
 import PingStatus from '../PingStatus.svelte';
 import ReloadPanel from '../ReloadPanel.svelte';
 import WidgetSlot from '../WidgetSlot.svelte';
 import ProfileManagerPanel from './ProfileManagerPanel.svelte';
</script>

<div class="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1.2fr)_minmax(0,0.8fr)]">
 <div class="space-y-4">
  <ProfileManagerPanel />
  <ReloadPanel />
 </div>
 <div class="space-y-4">
  <section class="rounded-lg border border-surface-200-800 bg-surface-100-900 p-4">
   <h2 class="mb-2 text-sm font-semibold opacity-80">接続情報</h2>
   <PingStatus />
  </section>
  <WidgetSlot
   title="開発者向けユーティリティ"
   subtitle="γ-6 予定"
   placeholder="ログダウンロード、ランタイム診断、ニッチな操作（broadcasters の force_remove 等）を整理します。"
  />
 </div>
</div>

