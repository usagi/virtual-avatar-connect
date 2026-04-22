<script lang="ts">
 /**
  * Logs タブ（γ-1）。
  *
  * 既存の EventStream + SnapshotView をそのまま並べ、γ-5 で高度フィルタ・ダウンロード・全文検索へ拡張する。
  */
 import EventStream from '../EventStream.svelte';
 import SnapshotView from '../SnapshotView.svelte';
</script>

<div class="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
 <SnapshotView />
 <EventStream />
</div>
