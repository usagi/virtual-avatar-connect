<script lang="ts">
 /**
  * Tools タブ（γ-1）。
  *
  * 現状は γ-6 までの中継として、開発者向けの ping/whoami 情報や今後の小物を集める領域。
  */
 import PingStatus from '../PingStatus.svelte';
 import WidgetSlot from '../WidgetSlot.svelte';
</script>

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
