<script lang="ts">
 /**
  * Logs タブ（γ-1）。
  *
  * 既存の EventStream + SnapshotView をそのまま並べ、γ-5 で高度フィルタ・ダウンロード・全文検索へ拡張する。
  */
 import EventStream from '../EventStream.svelte';
 import EventHistoryPanel from '../EventHistoryPanel.svelte';
 import SnapshotView from '../SnapshotView.svelte';
</script>

<section class="grid gap-4">
 <div class="flex flex-wrap items-start justify-between gap-3">
  <div>
   <h2 class="text-xl font-semibold">Observability</h2>
   <p class="text-sm opacity-65">Runtime snapshot, live event timeline, and operational evidence.</p>
  </div>
  <div class="vac-panel grid grid-cols-3 overflow-hidden text-center text-xs">
  <div class="border-r border-surface-200-800 px-3 py-2">
    <div class="font-semibold">Snapshot</div>
    <div class="opacity-60">state</div>
   </div>
   <div class="border-r border-surface-200-800 px-3 py-2">
    <div class="font-semibold">Timeline</div>
    <div class="opacity-60">events</div>
   </div>
   <div class="px-3 py-2">
    <div class="font-semibold">Filters</div>
    <div class="opacity-60">kinds</div>
   </div>
  </div>
 </div>

 <div class="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(26rem,0.92fr)_minmax(28rem,1.08fr)]">
  <div class="min-w-0">
   <SnapshotView />
  </div>
  <div class="min-w-0">
   <div class="grid gap-4">
    <EventStream />
    <EventHistoryPanel />
   </div>
  </div>
 </div>
</section>
